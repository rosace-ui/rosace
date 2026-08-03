# Platform Channel

Sooner or later your app needs something ROSACE doesn't have a widget or API for — the battery level, a native camera permission prompt, a third-party SDK that only ships a Swift/Kotlin package. Platform Channel is the bridge for exactly that: a named, two-way pipe between your Rust code and a few lines of native code you write yourself, in the style of Flutter's `MethodChannel`.

There are two directions, and picking the right one matters.

## Rust asks native for something (the common case)

This is `invoke_method` — use it any time Rust wants an answer from native code, even if that answer is normally instant. It always returns right away with a reactive handle, so you never block the UI thread waiting on native (which might be showing a system dialog and waiting on the user).

```rust
use rosace::prelude::*;
use rosace_ffi::ChannelCallState;
use serde_json::Value;

struct BatteryDemo;

impl Component for BatteryDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let call: Atom<Option<Atom<ChannelCallState>>> = ctx.state(None);

        let status = match call.get().as_ref().map(|a| a.get()) {
            None => Text::new("Not asked yet."),
            Some(ChannelCallState::Pending) => Text::new("Asking native…"),
            Some(ChannelCallState::Resolved(v)) => Text::new(format!("Battery: {v}%")),
            Some(ChannelCallState::Failed(e)) => Text::new(format!("Error: {e}")),
        };

        let c = call.clone();
        Scaffold::new(
            Column::new()
                .child(Button::new("Get Battery Level").on_press(move || {
                    let result = rosace_ffi::invoke_method(
                        "dev.example.myapp/battery",
                        "getLevel",
                        Value::Null,
                    );
                    // set_always, not set — Atom<ChannelCallState> can't
                    // implement PartialEq (it's a handle, not a value), so
                    // the usual equal-write dedup isn't available here.
                    c.set_always(Some(result));
                }))
                .child(status),
        )
        .into_element()
    }
}
```

`invoke_method` mints a fresh `Atom<ChannelCallState>` and returns it holding `Pending`. Reading `.get()` on it inside `build()` subscribes your component — the moment native reports back, the UI updates on its own. No polling code to write.

## Native asks Rust for something (the rare case)

This is the reverse: native code (a home-screen widget, a Siri Shortcut, a notification action — anything outside your ROSACE UI) needs an answer from your Rust logic *right now*. Register a handler once, at startup:

```rust
// in lib.rs
pub(crate) fn app_init() {
    rosace_ffi::set_method_call_handler("dev.example.myapp/math", Box::new(|method, args| {
        match method {
            "add" => {
                let nums: Vec<i64> = serde_json::from_value(args)
                    .map_err(|e| format!("expected a JSON array: {e}"))?;
                Ok(serde_json::Value::from(nums.iter().sum::<i64>()))
            }
            other => Err(format!("unknown method '{other}'")),
        }
    }));
}
```

> **Register in `app_init()`, not `launch()`.** `rsc new` generates both — `launch()` is desktop/web only; mobile's native entry points (`rsc_engine_init`, `nativeInit`) construct the engine directly and never call `launch()`. A handler registered only in `launch()` will silently never exist on iOS/Android. `app_init()` is called from *every* platform's entry point specifically so this can't happen — always put one-time setup there.

Your handler must answer fast — it runs inline on whatever native thread called in, blocking it until you return. If the real work might take a moment, that's the *other* direction (native queues nothing here; there's nowhere for native to "come back later").

## Wiring the native side

`rsc new` generates the five FFI exports (`rsc_platform_channel_take_outgoing`/`_report_result`/`_report_error`/`_dispatch` + `rsc_string_free`) and a `pollPlatformChannel()` that already recognizes push-permission discovery. Add your own channel by extending it — iOS:

```swift
// EngineViewController.swift, inside pollPlatformChannel's switch:
case ("dev.example.myapp/battery", "getLevel"):
    UIDevice.current.isBatteryMonitoringEnabled = true
    let level = Int(UIDevice.current.batteryLevel * 100)
    "\(level)".withCString { rsc_platform_channel_report_result(callId, $0) }
```

Android:

```kotlin
// MainActivity.kt, inside pollPlatformChannel's when:
channel == "dev.example.myapp/battery" && method == "getLevel" -> {
    val bm = getSystemService(Context.BATTERY_SERVICE) as BatteryManager
    val level = bm.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)
    nativePlatformChannelReportResult(callId, level.toString())
}
```

To trigger the reverse direction from native (proving your `set_method_call_handler` really works), call `rsc_platform_channel_dispatch(channel, method, argsJson)` directly — it returns the result synchronously, in one call.

## A worked example

The `platform_channel_demo` and `showcase` apps in [rosace-examples](https://github.com/rosace-ui/rosace-examples) are complete, real, running showcases of both directions — including a real camera-permission prompt built on `rosace_ffi::request_camera` (already built into ROSACE — see [Persistence & Networking](persistence-networking.md) for the sibling pattern of a capability that's ready to use without any wiring) and a boot-time self-test proving the native→Rust direction with a log line you can watch in Xcode's console or `adb logcat`.

---

**Under the hood:** the queue, the atom-based reactivity, and the memory-ownership rules for strings crossing the FFI boundary are covered in the architecture book — see [Platform Channel](../architecture/platform-channel.md).

Next: [Adapting a Rust Library](rust-library-adapters.md).
