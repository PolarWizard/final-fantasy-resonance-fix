#pragma once
#include "rust/cxx.h"
#include <cstdint>

/// @file
/// The C++ side of the cxx bridge: every function Rust calls, and forward
/// declarations of the shared structs cxx generates. Definitions live with the
/// fix they belong to -- layout.cpp for the arithmetic, hud.cpp and movies.cpp
/// for the corrections.

namespace ffrs {

/// @name Layout arithmetic
/// Pure functions, defined in layout.cpp and exercised from src/tests/layout.rs.
/// @{
struct Inset;
Inset letterbox_inset(double width, double height, double aspect) noexcept;
struct OffsetState;
float apply_offset(OffsetState& state, float current, float inset) noexcept;
struct Scale;
Scale fit_16_9(double width, double height) noexcept;
Scale full_bleed_scale(double box_width, double box_height, double screen_width,
                       double screen_height) noexcept;
/// @}

/// @name HUD corrections
/// Defined in hud.cpp. Each takes a widget address out of a hook's registers.
/// @{
rust::String constrain_hud_slot(std::size_t widget, std::size_t slot, std::uint32_t path,
                                double aspect);
rust::String correct_tooltip_position(std::size_t widget, double aspect);
rust::String expand_menu_backdrop(std::size_t widget);
rust::String note_capture_target(std::size_t context, std::size_t target);
rust::String reframe_capture(std::size_t widget);
rust::String update_map_backdrop(std::size_t widget, std::int32_t visibility);
/// @}

/// @name Movie corrections
/// Defined in movies.cpp. Each takes a widget address out of a hook's registers.
/// @{
rust::String wrap_movie_root(std::size_t widget);
rust::String on_battle_sequence_event(std::size_t parameter);
void sync_movie_backdrop(std::size_t widget, std::uint8_t visibility);
rust::String on_movie_transform(std::size_t widget);
rust::String on_native_movie(std::size_t object, bool is_screen);
/// @}

} // namespace ffrs
