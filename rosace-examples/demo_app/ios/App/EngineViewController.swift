//! Drives the ROSACE engine through the `rosace-ffi` C boundary
//! (`rosace-ffi/include/rsc_engine.h`) — a CAMetalLayer-backed view,
//! init/resize/input/frame calls, and real `UIView.safeAreaInsets` feeding
//! `rosace_core::SafeArea` (replacing the old winit outer/inner-size
//! workaround from Phase 20-22).

import UIKit
import QuartzCore

// MARK: - FFI declarations (mirrors rosace-ffi/include/rsc_engine.h;
// no bridging header needed — matches the pattern proven in
// rosace-ffi/examples/ios_stub.rs's Simulator verification).

typealias RscEngine = OpaquePointer

struct RscInputEvent {
    var kind: UInt32
    var x: Float
    var y: Float
    var button: UInt32
    var key: UInt32
    var character: UInt32
    var width: UInt32
    var height: UInt32
    var delta_x: Float
    var delta_y: Float
}

private let RSC_EVENT_MOUSE_MOVE: UInt32 = 0
private let RSC_EVENT_MOUSE_DOWN: UInt32 = 1
private let RSC_EVENT_MOUSE_UP: UInt32 = 2
private let RSC_BUTTON_LEFT: UInt32 = 0

@_silgen_name("rsc_engine_init")
func rsc_engine_init(_ surfaceHandle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32, _ scale: Float) -> RscEngine?

@_silgen_name("rsc_engine_resize")
func rsc_engine_resize(
    _ engine: RscEngine?, _ width: UInt32, _ height: UInt32, _ scale: Float,
    _ safeTop: Float, _ safeRight: Float, _ safeBottom: Float, _ safeLeft: Float
)

@_silgen_name("rsc_engine_input")
func rsc_engine_input(_ engine: RscEngine?, _ events: UnsafePointer<RscInputEvent>?, _ count: Int)

@_silgen_name("rsc_engine_frame")
func rsc_engine_frame(_ engine: RscEngine?)

@_silgen_name("rsc_engine_shutdown")
func rsc_engine_shutdown(_ engine: RscEngine?)

// MARK: - View

/// A `CAMetalLayer`-backed view — the surface the Rust engine renders into.
///
/// `contentsScale` is set explicitly in `init` — UIKit only auto-syncs a
/// view's OWN default `CALayer` to the screen's pixel density; overriding
/// `layerClass` with a custom layer (as this does) opts out of that
/// automatic behavior, and a `CAMetalLayer` left at its default
/// `contentsScale = 1.0` renders a blurry, effectively-downscaled image
/// even though the Rust side correctly renders at full physical-pixel
/// resolution — one of the most common CAMetalLayer gotchas. Root-caused
/// and fixed 2026-07-08 after a direct visual report of blurry text.
final class MetalView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    override init(frame: CGRect) {
        super.init(frame: frame)
        (layer as! CAMetalLayer).contentsScale = UIScreen.main.scale
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        (layer as! CAMetalLayer).contentsScale = UIScreen.main.scale
    }
}

final class EngineViewController: UIViewController {
    private var engine: RscEngine?
    private var displayLink: CADisplayLink?

    override func loadView() {
        view = MetalView(frame: UIScreen.main.bounds)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        let scale = Float(view.contentScaleFactor)
        let width = UInt32(view.bounds.width * CGFloat(scale))
        let height = UInt32(view.bounds.height * CGFloat(scale))
        let viewPtr = Unmanaged.passUnretained(view).toOpaque()
        engine = rsc_engine_init(viewPtr, width, height, scale)

        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .default)
        displayLink = link
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        guard let engine else { return }
        let scale = Float(view.contentScaleFactor)
        let width = UInt32(view.bounds.width * CGFloat(scale))
        let height = UInt32(view.bounds.height * CGFloat(scale))
        let insets = view.safeAreaInsets
        rsc_engine_resize(
            engine, width, height, scale,
            Float(insets.top), Float(insets.right), Float(insets.bottom), Float(insets.left)
        )
    }

    @objc private func tick() {
        guard let engine else { return }
        rsc_engine_frame(engine)
    }

    // MARK: Touch -> MouseDown/MouseMove/MouseUp (same convention the
    // existing winit `Touch` handling and `RscInputEventFfi` conversion use
    // — no separate touch event kind needed).

    private func send(kind: UInt32, touches: Set<UITouch>) {
        guard let engine, let touch = touches.first else { return }
        let p = touch.location(in: view)
        var event = RscInputEvent(
            kind: kind, x: Float(p.x), y: Float(p.y), button: RSC_BUTTON_LEFT,
            key: 0, character: 0, width: 0, height: 0, delta_x: 0, delta_y: 0
        )
        withUnsafePointer(to: &event) { rsc_engine_input(engine, $0, 1) }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: RSC_EVENT_MOUSE_DOWN, touches: touches)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: RSC_EVENT_MOUSE_MOVE, touches: touches)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: RSC_EVENT_MOUSE_UP, touches: touches)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: RSC_EVENT_MOUSE_UP, touches: touches)
    }

    deinit {
        displayLink?.invalidate()
        if let engine { rsc_engine_shutdown(engine) }
    }
}
