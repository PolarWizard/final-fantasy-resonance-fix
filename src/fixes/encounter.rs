//! The encounter transition's screen capture, re-framed to the shape it is
//! drawn at.

use std::{cell::Cell, ptr::null_mut};

use sdk::{
    CoreUObject::{FLinearColor, FVector2D, UObject},
    Engine::{
        EBlendMode, FDrawToRenderTargetContext, UCanvas, UKismetRenderingLibrary, UTexture,
        UTextureRenderTarget2D,
    },
    UMG::UWidget,
    rt::NameCache,
};

use super::widgets::{approximately, arranged, live, viewport_local_size};
use crate::utils::{ModuleInfo, SignatureHook, inject_hook, log_report};

thread_local! {
    // The target the back buffer capture was last handed, a context that
    // resolves the world for the draws below, and whether a capture is waiting
    // to be re-framed. Identities are re-checked before use rather than
    // trusted across frames.
    static CAPTURE_TARGET: Cell<*mut UTextureRenderTarget2D> = const { Cell::new(null_mut()) };
    static CAPTURE_CONTEXT: Cell<*mut UObject> = const { Cell::new(null_mut()) };
    static PENDING: Cell<bool> = const { Cell::new(false) };
}

/// Remembers the render target `CPP_BackBufferCaptureSubsystem::Capture` is
/// handed, and that a fresh frame is on its way into it.
///
/// The whole function is two writes -- the target at +0x28 and the request word
/// 0x0100 at +0x30 -- and the signature matches their shape, unique image-wide
/// with the field offsets, registers and branch wildcarded and the request
/// value literal. Nothing is written to the game from here: the copy the
/// request raises is deferred, so the frame is re-framed afterwards, from the
/// widget that draws it.
pub(super) fn install(module: &ModuleInfo) {
    let capture = SignatureHook {
        tag: "Back buffer capture",
        signature: "48 8B ?? ?? 48 85 C0 74 ?? 48 89 ?? ?? 66 C7 ?? ?? 00 01 C3",
        offset: 0,
    };
    inject_hook(module, &capture, |ctx| {
        // RCX is the subsystem, RDX the render target it was handed.
        note_capture_target(ctx.rcx as *mut UObject, ctx.rdx as *mut _);
    });
}

/// Records the target the capture was handed, since the request this raises is
/// what tells [`reframe_capture`] a fresh frame is on the way.
fn note_capture_target(subsystem: *mut UObject, target: *mut UTextureRenderTarget2D) {
    if !live(subsystem) || !live(target) {
        return;
    }
    CAPTURE_CONTEXT.set(subsystem);
    CAPTURE_TARGET.set(target);
    PENDING.set(true);
    let viewport = viewport_local_size(subsystem);
    let target = unsafe { &*target };
    log_report(
        "capture_target",
        format!(
            "Capture target: {} is {}x{}, viewport {}x{}",
            target.GetName(),
            target.SizeX,
            target.SizeY,
            viewport.X,
            viewport.Y
        ),
    );
}

/// The shape of the frame the encounter transition draws, as width over height.
///
/// The capture's own target decides it: the copy scales whatever it is given
/// into that target, so the target's proportions are the proportions of the
/// picture the transition presents. 16:9 until a capture is seen.
pub(super) fn captured_aspect() -> f64 {
    let target = CAPTURE_TARGET.get();
    if live(target) && unsafe { (*target).SizeY } > 0 {
        let target = unsafe { &*target };
        f64::from(target.SizeX) / f64::from(target.SizeY)
    } else {
        16.0 / 9.0
    }
}

/// Draws a region of `source` across the whole of `destination`.
///
/// A canvas is the one route from here to a render target's pixels, and it
/// takes the source rectangle in UV and the destination rectangle in pixels, so
/// a crop and a stretch are a single draw. `origin` is where in `source`
/// sampling starts and `extent` how much of it is sampled, both in UV.
///
/// Returns whether the canvas was handed out and the draw issued.
fn draw_region(
    context: *mut UObject,
    destination: *mut UTextureRenderTarget2D,
    source: *mut UTexture,
    origin: FVector2D,
    extent: FVector2D,
) -> bool {
    let mut canvas: *mut UCanvas = null_mut();
    let mut size = FVector2D::new(0.0, 0.0);
    let mut draw: FDrawToRenderTargetContext = unsafe { std::mem::zeroed() };
    UKismetRenderingLibrary::BeginDrawCanvasToRenderTarget(
        context,
        destination,
        Some(&mut canvas),
        Some(&mut size),
        Some(&mut draw),
    );
    if !live(canvas) {
        return false;
    }
    let white = FLinearColor {
        R: 1.0,
        G: 1.0,
        B: 1.0,
        A: 1.0,
    };
    let zero = FVector2D::new(0.0, 0.0);
    unsafe {
        (*canvas).K2_DrawTexture(
            source,
            &zero,
            &size,
            &origin,
            &extent,
            &white,
            EBlendMode::BLEND_Opaque,
            0.0,
            &zero,
        );
    }
    // The destination stays bound to the canvas until this call.
    UKismetRenderingLibrary::EndDrawCanvasToRenderTarget(context, &draw);
    true
}

/// The scratch target the re-frame reads from, created fresh for each capture.
///
/// A draw cannot sample the target it writes, so the captured frame is copied
/// aside and the crop is taken from the copy. Nothing in the game holds this
/// object, so collection may take it between transitions, and a pointer kept
/// from the last one may point at freed memory that `live` cannot tell apart
/// from a live object. Creating one per capture -- once per transition --
/// never touches a collected target, and the one made here is used within the
/// same call, before collection can run.
///
/// Returns a target in the capture's own size and format, or null.
fn bounce_target(
    context: *mut UObject,
    target: *mut UTextureRenderTarget2D,
) -> *mut UTextureRenderTarget2D {
    let target = unsafe { &*target };
    let scratch = UKismetRenderingLibrary::CreateRenderTarget2D(
        context,
        target.SizeX,
        target.SizeY,
        target.RenderTargetFormat,
        &target.ClearColor,
        false,
        false,
    );
    if live(scratch) { scratch } else { null_mut() }
}

/// Makes the captured frame a true picture of the capture's own shape.
///
/// The copy behind `CPP_BackBufferCaptureSubsystem::Capture` scales the whole
/// screen into a target that is 16:9, so on a wider screen the frame arrives
/// horizontally compressed -- an exported capture measured half width. The
/// transition draws that frame as a picture of the target's shape, which is how
/// the compression reaches the screen.
///
/// So the middle of the capture, the part its own shape covers, is stretched
/// back over the whole target: its contents become a picture in its own
/// proportions again, and what the sides lose is width a 16:9 screen never had.
/// Two draws, because a canvas cannot sample the target it writes -- the frame
/// goes out to the scratch whole and comes back cropped.
///
/// Run once per capture, from the transition's own image, which is the first
/// thing to touch the target after the copy the request raised.
///
/// # Safety
///
/// `widget` is the widget in RCX at native UpdateRenderTransform, on the game
/// thread.
pub(super) unsafe fn reframe_capture(widget: *mut UWidget) {
    static CAPTURE_NAME: NameCache = NameCache::new("Image_44");
    let (target, context) = (CAPTURE_TARGET.get(), CAPTURE_CONTEXT.get());
    // One attempt per capture: whatever happens below, the next Image_44 (the
    // menus' dimming image shares the name) waits for the next capture.
    if !live(widget)
        || unsafe { &*widget }.Name != CAPTURE_NAME.get()
        || !PENDING.replace(false)
        || !live(target)
        || !live(context)
    {
        return;
    }
    let viewport = viewport_local_size(context);
    if !arranged(&viewport) {
        return;
    }
    // The fraction of the captured width the capture's own shape covers: half
    // of it at 32:9 against a 16:9 capture, and all of it on a screen no wider
    // than the capture, where there is nothing to trim.
    let keep = (captured_aspect() * viewport.Y / viewport.X).min(1.0);
    if approximately(keep, 1.0) {
        return;
    }
    let bounce = bounce_target(context, target);
    let whole = FVector2D::new(1.0, 1.0);
    if bounce.is_null()
        || !draw_region(
            context,
            bounce,
            target.cast(),
            FVector2D::new(0.0, 0.0),
            whole,
        )
    {
        return;
    }
    draw_region(
        context,
        target,
        bounce.cast(),
        FVector2D::new((1.0 - keep) / 2.0, 0.0),
        FVector2D::new(keep, 1.0),
    );
    let target = unsafe { &*target };
    log_report(
        "reframe_capture",
        format!(
            "Re-framed capture: kept {keep} of {}x{} at aspect {} on {}x{}",
            target.SizeX,
            target.SizeY,
            captured_aspect(),
            viewport.X,
            viewport.Y
        ),
    );
}
