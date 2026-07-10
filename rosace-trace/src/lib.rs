pub mod bus;
pub mod event;
pub mod subscribers;

pub use bus::{TraceSubscriber, TracingBus, TRACING_BUS};
pub use event::RosaceTrace;

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
