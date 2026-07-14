# Phase 29 — App Lifecycle + Push Notifications (D110)

> Status: BOTH STEPS LANDED + live-verified on the iOS Simulator.
> Named account-blocked deferrals on Step 2: real APNs token/network
> push (needs the Apple Developer team) and Android FCM (needs a
> Firebase project) — see Step 2's landed note. Android lifecycle
> (Step 1's other half) awaits an emulator session.
> Started: 2026-07-14
> Completed: 2026-07-14 (with the named deferrals above)
> Decision: **D110** — wire real app lifecycle state (resume/pause/
> background/suspend) and push-notification registration/delivery across
> the existing D106 native-host FFI bridge, reusing the exact
> request-queue + result-atom + host-native-call shape Phase 24 Step 5
> already proved with camera permission. Queued after Phase 27 (GPU
> rendering) and Phase 28 (TextInput/IME), per the same-day sequencing
> discussion — technically independent of both, but the priority order
> stands.

## Why This Phase

Raised 2026-07-10 while discussing how Phase 27's GPU work and the
existing mobile architecture (D106) interact. Checking the actual code
(not the plan) found two real, unimplemented gaps in what a shippable
mobile app needs:

- **`D042`'s app-lifecycle decision was never built.** `GlobalAtom<LifecycleState>`
  (Active/Inactive/Background/Suspended) + `use_app_lifecycle()` is
  recorded in `DECISIONS.md` but a workspace-wide grep for
  `LifecycleState`/`use_app_lifecycle` returns nothing — it's a decision
  on paper, zero implementation.
- **No lifecycle or push-notification event crosses the FFI bridge.**
  `rosace-ffi/src/event.rs`'s event kinds are only
  `MouseMove/Down/Up`, `KeyDown/Up`, `Text`, `WindowResized`, `Scroll`.
  Phase 24 Step 5 named "push-notification registration" as one
  *candidate* proof of the native-host model; camera permission got built
  instead (`rosace-ffi/src/capability.rs`), so push notifications were
  never actually reached.

The native-host architecture that makes this reachable at all (D106,
Phase 24, complete) exists specifically because winit cannot own the iOS
process (`UIApplicationMain` generates its own implicit `AppDelegate`) —
these callbacks were structurally unreachable before that phase landed.
Now that a real, editable `AppDelegate`/`SceneDelegate`/`MainActivity.kt`
exists, adding lifecycle and push notifications is "just another message
over the bridge," per Phase 24 Step 5's own scope note — this phase is
that message, done for real.

## Out of Scope (deliberately, not silently dropped)

- **Rich push payloads / notification actions / silent push / badge
  management.** Step 2 below proves basic registration + foreground
  delivery of a push notification into Rust app code. Notification
  categories, actions, and background-fetch-on-push are real follow-up
  work once the basic path is proven, not part of this phase's exit bar.
- **A general `Permission`/`Haptics`/`Biometrics`/`use_sensor()` capability
  surface.** Phase 24 Step 5's scope note already deferred this
  explicitly — this phase adds exactly two more capabilities
  (lifecycle, push) to the existing bridge, not a general framework for
  arbitrary future capabilities. That broader surface is its own future
  phase once enough real capabilities exist to see the actual shared shape.
- **Desktop/web lifecycle.** `AppState::resumed()` already exists for
  desktop's winit lifecycle (used to init `GpuPresenter`) — this phase is
  scoped to the mobile gap (background/suspend/push), not a desktop
  rework. Web has no meaningful "background" state in the same sense
  (page visibility API is a different, smaller problem) — out of scope
  here, revisit only if a real need surfaces.

## Steps

### Step 1 — Decide `LifecycleState`'s real home + wire it through the FFI bridge
`D042` never picked a crate for `LifecycleState`. Resolve that first
(likely `rosace-core` alongside other cross-cutting state, or a new
capability module in `rosace-ffi` mirroring `capability.rs`'s shape —
decide based on whether desktop/web should ever set it too, not just
mobile). Add new FFI event kinds (`TZR_EVENT_LIFECYCLE_ACTIVE` /
`_INACTIVE` / `_BACKGROUND` / `_SUSPENDED`, or a dedicated
`tzr_engine_lifecycle(handle, state)` call — pick whichever fits
`event.rs`'s existing flat-struct convention better). iOS:
`AppDelegate.swift`/`SceneDelegate.swift` template calls it from
`applicationDidBecomeActive`/`applicationWillResignActive`/
`applicationDidEnterBackground`/`applicationWillTerminate`. Android:
`MainActivity.kt` template calls it from `onResume`/`onPause`/`onStop`.

Exit: a real running app on-device (or simulator/emulator) backgrounds
and resumes; a widget reading `LifecycleState` via `use_app_lifecycle()`
observably re-renders with the correct state — proven live, not just
compiled.

**Landed 2026-07-14 (commits ea4b9aa + the D120 rename that immediately
followed)**. Home resolved: `rosace-core/src/app_lifecycle.rs` (NOT
`rosace-platform` as D042 originally said — platform is unreachable from
component code; core is the lowest common layer both the FFI setter side
and the component reader side already depend on, the exact `ime_hint.rs`
precedent). `LifecycleState` (Active default/Inactive/Background/
Suspended) + `use_app_lifecycle(ctx)` (reads AND explicitly subscribes
the component — `GlobalAtom`s aren't auto-subscribed by hook machinery;
`FormField::for_ctx` convention) + `app_lifecycle()` (non-subscribing
read for engine/host/watcher-thread code) + `set_app_lifecycle()`. Atom
id `0xFFF9` (next in the reserved-high-id ladder). FFI: four flat event
kinds `RSC_EVENT_LIFECYCLE_ACTIVE/INACTIVE/BACKGROUND/SUSPENDED`
(8/9/10/11) → `InputEvent::Lifecycle(LifecycleState)`;
`FrameEngine`'s dispatch writes the atom. **Design point found while
building, not in the plan**: `Engine::input` applies lifecycle events
IMMEDIATELY as well as queueing them — iOS pauses the display link in
background (and background Metal work is prohibited), so a purely
frame-queued `Background` event would first be seen on RESUME, the exact
opposite of "pause work while backgrounded". iOS template:
`UIApplication` notification observers in `EngineViewController` (owns
the engine handle — no AppDelegate/SceneDelegate plumbing). Android
template: `onResume`/`onPause`/`onStop` → new `nativeLifecycle` JNI fn;
no SUSPENDED on Android (no reliable pre-kill callback). Tests: core
unit tests (default/round-trip/subscription-marks-dirty), FFI mapping
round-trip, headless `FrameEngine` integration test asserting the idle
frame does NOT rebuild (so the re-render assertion can't false-positive)
and the event-carrying frame does.

**Live exit-bar proof 2026-07-14, iOS Simulator (iPhone 15 Pro, 17.4)**:
fresh `rsc new lifecycle_proof --platforms ios` scaffold (also the first
end-to-end run of the D120-renamed `rsc_*` ABI), `rsc run --target ios`,
app shows `use_app_lifecycle()` live; backgrounded via launching
Settings, resumed via `simctl launch` (same pid — process survived);
UI history renders **Active → Inactive → Background → Active**, recorded
at SET time by an app-side watcher thread polling `app_lifecycle()`
(itself the D110 "pause expensive work" pattern) — build-time logging
alone would miss `Background`, since no frames run while backgrounded.
Android half of the template compiles in codegen tests; live
emulator verification folds into the next real Android device session
(same discipline as Phase 24/28's device-deferred halves).

### Step 2 — Push-notification registration + foreground delivery
Mirror `capability.rs`'s exact three-piece shape: `request_push_permission()`
queues a request, the native host polls it once per frame tick (same
pattern `Engine::frame` already uses), triggers the real native API
(`UNUserNotificationCenter.requestAuthorization` + APNs token retrieval
on iOS, `FirebaseMessagingService`/`POST_NOTIFICATIONS` permission on
Android), and `report_push_result`/`report_push_token` writes a
`GlobalAtom` app code can read. A received notification while the app is
foregrounded delivers into Rust the same way (a queued "event" the host
reports on the next capability poll), driving a UI re-render.

Exit: a demo app requests push permission, receives and displays a real
device token, and a real test push notification (sent via APNs/FCM to
that token) is received and observably handled by Rust app code while the
app is in the foreground — proven live on-device, same discipline as
every prior phase's exit bar.

**Landed 2026-07-14.** `capability.rs` gained the second capability in
the exact camera shape plus two new pieces camera didn't need:
`PUSH_PERMISSION` (`0xFFF8`), `PUSH_TOKEN` (`0xFFF7`, tokens rotate —
later reports win), and `PUSH_MESSAGE` (`0xFFF6`) — latest-wins by
design with a receipt `seq` so two identical payloads still re-render
subscribers (`request_push_permission`/`take_push_request`/
`report_push_result`/`report_push_token`/`report_push_notification`).
C ABI: `rsc_push_permission_take_request/_report_result`,
`rsc_push_report_token`, `rsc_push_report_notification` (C strings,
null-safe) — declared in `rsc_engine.h`, exported by `ios_stub.rs` and
the generated per-app `ffi.rs`. iOS template: `AppDelegate` is now the
`UNUserNotificationCenterDelegate` (set in `didFinishLaunching`, before
any notification can arrive) reporting foreground `willPresent`
deliveries + `didRegisterForRemoteNotificationsWithDeviceToken` (hex
token) / graceful `didFail` logging; `EngineViewController.tick()` polls
`take_request` once per frame and drives the real
`UNUserNotificationCenter.requestAuthorization` →
`registerForRemoteNotifications` on grant.

**Live verification 2026-07-14, iOS Simulator, fresh `rsc new push_proof`
scaffold (zero hand-patching)**: the REAL OS permission dialog appeared
(request path: Rust queue → frame-tick poll → `requestAuthorization`);
the user's Allow tap flowed back to `PUSH_PERMISSION` and the widget
re-rendered "GRANTED"; a payload with custom keys delivered via
`xcrun simctl push` (the OS's real notification-daemon delivery path)
landed through `willPresent` → `rsc_push_report_notification` →
`PUSH_MESSAGE`, and the widget rendered title/body/full payload JSON.
APNs registration was exercised and failed GRACEFULLY as designed (no
signing team/`aps-environment` entitlement on the simulator) — the token
atom stays `None`, the UI says so, nothing crashes.

**Named deferrals (account-blocked, not silently dropped)**: (1) a real
APNs device token + a push over the actual APNs network — needs the
user's Apple Developer team (signing + APNs key); the `simctl push` hop
covers everything after the network. (2) Android push entirely — FCM
requires a real Firebase project (`google-services.json`, gradle
plugin); templating that blind with nothing to verify against violates
the D106 device-session discipline, so the Android host template gains
push wiring in the session that has a Firebase project to test with.
Both fold into the next real device/account session.

## Sequencing

Step 1 and Step 2 are independent of each other (different capabilities,
same bridge pattern) and can be done in either order, but Step 1 is
smaller and lower-risk (no external push service dependency), so it's the
natural first step. Both steps are independent of Phase 27 (rendering)
and Phase 28 (TextInput/IME) — the only reason this phase is queued after
both is the user's explicit same-day prioritization, not a technical
dependency.

## Migration Rule

Purely additive — no existing widget, FFI symbol, or native-host template
behavior changes for apps that don't opt into lifecycle/push handling.
