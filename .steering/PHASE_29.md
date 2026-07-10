# Phase 29 — App Lifecycle + Push Notifications (D110)

> Status: Scoped, not started.
> Started: —
> Completed: —
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
