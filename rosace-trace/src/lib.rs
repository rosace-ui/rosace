pub mod bus;
pub mod event;
pub mod log;
pub mod subscribers;

pub use bus::{TraceSubscriber, TracingBus, TRACING_BUS};
pub use event::{LogLevel, RosaceTrace, TraceCategory};
pub use log::{init_from_env, max_level, set_max_level};
pub use subscribers::{flight_recorder, install_flight_recorder, install_log_console};
pub use subscribers::perfetto::to_chrome_trace_json;

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::bus::{TraceSubscriber, TRACING_BUS};
    use crate::event::{ComponentId, RosaceTrace};
    use crate::subscribers::ring_buffer::RingBufferSubscriber;
    use crate::{location, trace};

    /// Test-only subscriber that counts received events.
    struct CountSubscriber(Arc<AtomicUsize>);

    impl TraceSubscriber for CountSubscriber {
        fn on_trace(&self, _event: &RosaceTrace) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn trace_emits_in_debug() {
        let counter = Arc::new(AtomicUsize::new(0));
        let sub = Arc::new(CountSubscriber(counter.clone()));
        TRACING_BUS.add_subscriber(sub);

        let before = counter.load(Ordering::SeqCst);

        trace!(RosaceTrace::ComponentMount {
            id: ComponentId(1),
            name: "TestComponent",
            location: location!(),
        });

        let after = counter.load(Ordering::SeqCst);
        assert_eq!(after - before, 1);

        TRACING_BUS.clear_subscribers();
    }

    #[test]
    fn ring_buffer_captures_events() {
        let buf = Arc::new(RingBufferSubscriber::new(1000));
        TRACING_BUS.add_subscriber(buf.clone());

        trace!(RosaceTrace::ComponentUnmount {
            id: ComponentId(2),
            name: "AnotherComponent",
        });

        let snap = buf.snapshot();
        assert!(!snap.is_empty());

        TRACING_BUS.clear_subscribers();
    }

    #[test]
    fn ring_buffer_evicts_oldest_when_full() {
        let buf = RingBufferSubscriber::new(3);

        for i in 0..5u64 {
            buf.on_trace(&RosaceTrace::ComponentUnmount {
                id: ComponentId(i),
                name: "X",
            });
        }

        // Capacity 3 — first two were evicted.
        assert_eq!(buf.len(), 3);
        let snap = buf.snapshot();
        if let RosaceTrace::ComponentUnmount { id, .. } = &snap[0] {
            assert_eq!(id.0, 2);
        } else {
            panic!("unexpected event type");
        }
    }

    #[test]
    fn multiple_events_all_captured() {
        // Use a local bus to avoid interference from parallel tests sharing TRACING_BUS.
        use crate::bus::TracingBus;
        let local_bus = TracingBus::new();
        let buf = Arc::new(RingBufferSubscriber::new(1000));
        local_bus.add_subscriber(buf.clone());

        for i in 0..10u64 {
            local_bus.emit(RosaceTrace::AtomRead {
                atom: crate::event::AtomId(i),
                component: ComponentId(0),
            });
        }

        assert_eq!(buf.len(), 10);
    }
}

#[cfg(test)]
mod flight_recorder_tests {
    use crate::event::{AtomId, ComponentId, RosaceTrace, TraceValue};
    use crate::subscribers::ring_buffer::RingBufferSubscriber;
    use std::sync::Arc;

    /// The O1 exit bar (D123): a flight recorder built on the high-frequency
    /// filter captures meaningful events and NEVER the per-frame firehose
    /// that hung the earlier attempt.
    #[test]
    fn flight_recorder_keeps_meaningful_events_and_drops_the_firehose() {
        let rec = Arc::new(RingBufferSubscriber::filtered(100, |e| !e.is_high_frequency()));
        let bus = crate::bus::TracingBus::new();
        bus.add_subscriber(rec.clone());

        // Simulate a frame's worth of noise + two meaningful events.
        for _ in 0..500 {
            bus.emit(RosaceTrace::AtomRead { atom: AtomId(1), component: ComponentId(1) });
            bus.emit(RosaceTrace::FrameStart { frame: 0, timestamp: std::time::Instant::now() });
            bus.emit(RosaceTrace::FrameEnd {
                frame: 0, duration: std::time::Duration::from_millis(1), dropped: false,
            });
        }
        bus.emit(RosaceTrace::AtomWrite {
            atom: AtomId(2), old: TraceValue::Opaque, new: TraceValue::Opaque,
            by: ComponentId(3), location: crate::location!(),
        });
        bus.emit(RosaceTrace::ComponentMount {
            id: ComponentId(4), name: "Widget", location: crate::location!(),
        });

        let snap = rec.snapshot();
        assert_eq!(snap.len(), 2, "1500 high-frequency events must be dropped; only the 2 meaningful ones kept");
        assert!(snap.iter().all(|e| !e.is_high_frequency()));
    }
}
