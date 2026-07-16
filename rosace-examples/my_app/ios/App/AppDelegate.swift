//! Owns the app lifecycle — our own AppDelegate, not winit's implicit one
//! (this is the whole point of D106: winit's iOS backend calls
//! UIApplicationMain itself and generates an AppDelegate no host code can
//! reach, which blocks push notifications, deep links, and background
//! tasks).

import UIKit
import UserNotifications

// Push-notification FFI (D110 Phase 29 Step 2) — the app's own staticlib
// exports these (src/ffi.rs); same @_silgen_name mechanism as the engine
// calls in EngineViewController.swift.
@_silgen_name("rsc_push_report_token")
private func rsc_push_report_token(_ token: UnsafePointer<CChar>?)
@_silgen_name("rsc_push_report_notification")
private func rsc_push_report_notification(
    _ title: UnsafePointer<CChar>?, _ body: UnsafePointer<CChar>?, _ payload: UnsafePointer<CChar>?
)

@main
final class AppDelegate: UIResponder, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        // Foreground-delivery hook — must be set before any notification
        // can arrive, so it lives here, not behind the permission request.
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    func application(
        _ application: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let config = UISceneConfiguration(name: "Default", sessionRole: connectingSceneSession.role)
        config.delegateClass = SceneDelegate.self
        return config
    }

    // MARK: Push registration outcome (D110 Phase 29 Step 2)

    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        let token = deviceToken.map { String(format: "%02x", $0) }.joined()
        token.withCString { rsc_push_report_token($0) }
    }

    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        // A legitimate outcome (no aps-environment entitlement, Simulator
        // without a signing team, no network) — the permission result was
        // already reported; the token atom simply stays unset.
        NSLog("rosace push: APNs registration failed: \(error.localizedDescription)")
    }

    // MARK: Foreground delivery (D110 Phase 29 Step 2)

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        let content = notification.request.content
        var payload = "{}"
        if let userInfo = content.userInfo as? [String: Any],
           JSONSerialization.isValidJSONObject(userInfo),
           let data = try? JSONSerialization.data(withJSONObject: userInfo),
           let s = String(data: data, encoding: .utf8) {
            payload = s
        }
        content.title.withCString { t in
            content.body.withCString { b in
                payload.withCString { p in rsc_push_report_notification(t, b, p) }
            }
        }
        completionHandler([.banner, .sound])
    }
}
