//! Keeps the game running while its window does not have focus.

use crate::utils::{ModuleInfo, patch, pattern_scan};

/// Stops the game and the engine pausing when the window loses focus.
///
/// The game pauses itself: at startup it binds a handler to an application
/// activation event, and the handler asks the game's pause manager to pause
/// whenever the application deactivates. A probe hooked on the engine tick
/// showed the game thread still ticking ~200 times a second unfocused, with
/// IsGamePaused true in game (false on the title screen, where it is not
/// pausable), and the world kept running once this handler was patched.
///
/// The engine has two focus-loss behaviours of its own, each switched on by
/// config: idle mode (`t.IdleWhenNotForeground`), which stops ticking and
/// rendering, and `UEngine::bPauseOnLossOfFocus`, which pauses every local
/// player. Both are read from the packaged ini, which ships Oodle-compressed,
/// so both gates are opened too. Each patch is a no-op when its setting is off.
/// All three are byte patches on code the game has not run yet at DLL attach.
pub(super) fn apply(module: &ModuleInfo) {
    // The game's activation handler: CMP byte ptr [RAX+0x18A],3; JZ skips the
    // request, otherwise XOR DIL,1 turns "active" into "pause", LEA R8 loads
    // the handler's own requester key, and CALL hands both to the pause
    // manager, which pauses on the first request (SetGamePaused(true),
    // Delegate.Game.Pause). The JZ at +0 becomes a JMP, so focus changes never
    // request a pause while every other requester (menus, events) still does.
    // The inversion, key load and call are unique image-wide; the jump, key and
    // call displacements are wildcarded.
    const GAME: &str = "74 ?? 40 80 F7 01 4C 8D 05 ?? ?? ?? ?? 40 0F B6 D7 48 8B CB E8";
    if let Some(address) = pattern_scan(module.address, GAME) {
        patch(address, "EB");
        log::info!(
            "Background game pause: match +{:X}, byte +0 = EB",
            address - module.address.0 as usize
        );
    } else {
        log::error!("Background game pause: signature not found");
    }

    // FEngineLoop::Tick's inlined ShouldUseIdleMode: XOR SIL,SIL clears the
    // idle flag, CMP [RAX],R13D tests the cached t.IdleWhenNotForeground value
    // against zero, then CALL FApp::HasFocus; TEST AL,AL; JNZ. The JZ at +6 is
    // the setting-off exit; as a JMP it is always taken, so SIL stays clear and
    // the loop neither sleeps 0.1s nor skips the frame. Its displacement and
    // the call target are code-relative and wildcarded.
    const IDLE: &str = "40 32 F6 44 39 28 74 ?? E8 ?? ?? ?? ?? 84 C0 75";
    if let Some(address) = pattern_scan(module.address, IDLE) {
        patch(address + 6, "EB");
        log::info!(
            "Background idle: match +{:X}, byte +6 = EB",
            address - module.address.0 as usize
        );
    } else {
        log::error!("Background idle: signature not found");
    }

    // UEngine::OnLostFocusPause opens with TEST byte ptr [RCX+0xD7C],1 --
    // bPauseOnLossOfFocus, bit 0 of the byte at 0xD7C in the SDK's UEngine --
    // and returns without pausing anyone when it is clear. Its immediate at +6
    // becomes 0, so the flag never survives the test. It is the only test of
    // that byte image-wide.
    const PAUSE: &str = "F6 81 7C 0D 00 00 01";
    if let Some(address) = pattern_scan(module.address, PAUSE) {
        patch(address + 6, "00");
        log::info!(
            "Background pause: match +{:X}, byte +6 = 00",
            address - module.address.0 as usize
        );
    } else {
        log::error!("Background pause: signature not found");
    }
}
