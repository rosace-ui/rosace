package dev.rosace.demo_app

import android.app.Activity
import android.os.Bundle
import android.view.Choreographer
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView

class MainActivity : Activity(), SurfaceHolder.Callback {

    companion object {
        init { System.loadLibrary("demo_app") }
    }

    private external fun nativeInit(surface: Surface, width: Int, height: Int, scale: Float): Long
    private external fun nativeResize(
        handle: Long, width: Int, height: Int, scale: Float,
        safeTop: Float, safeRight: Float, safeBottom: Float, safeLeft: Float,
    )
    private external fun nativeTouch(handle: Long, kind: Int, x: Float, y: Float)
    private external fun nativeFrame(handle: Long)
    private external fun nativeShutdown(handle: Long)

    private var engineHandle: Long = 0
    private lateinit var surfaceView: SurfaceView

    private val frameCallback = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (engineHandle != 0L) {
                nativeFrame(engineHandle)
                Choreographer.getInstance().postFrameCallback(this)
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        surfaceView = SurfaceView(this)
        surfaceView.holder.addCallback(this)
        setContentView(surfaceView)
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        val scale = resources.displayMetrics.density
        val width = surfaceView.width
        val height = surfaceView.height
        engineHandle = nativeInit(holder.surface, width, height, scale)
        Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        if (engineHandle == 0L) return
        val scale = resources.displayMetrics.density
        // Basic safe-area: only the status bar height (systemWindowInsetTop),
        // not a full WindowInsets-driven cutout/gesture-nav treatment — a
        // known simplification (see .steering/CRATE_CONTRACTS.md Known
        // Issues), the Android counterpart of iOS's real UIView.safeAreaInsets
        // (Step 2) is follow-up work, not silently claimed equivalent here.
        nativeResize(engineHandle, width, height, scale, 0f, 0f, 0f, 0f)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        if (engineHandle == 0L) return
        nativeShutdown(engineHandle)
        engineHandle = 0
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (engineHandle == 0L) return false
        val kind = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> 1
            MotionEvent.ACTION_MOVE -> 0
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> 2
            else -> return false
        }
        nativeTouch(engineHandle, kind, event.x, event.y)
        return true
    }
}
