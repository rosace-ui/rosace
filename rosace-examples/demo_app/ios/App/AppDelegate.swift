//! Owns the app lifecycle — our own AppDelegate, not winit's implicit one
//! (this is the whole point of D106: winit's iOS backend calls
//! UIApplicationMain itself and generates an AppDelegate no host code can
//! reach, which blocks push notifications, deep links, and background
//! tasks).

import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        true
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
}
