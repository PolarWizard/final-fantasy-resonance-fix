#pragma once
#include <cstddef>

/// @file
/// Force-included ahead of every translation unit (see build.rs) to repair one
/// collision in the Dumper-7 output.
///
/// Assertions.inl defines DUMPER7_ASSERTS_FTextData twice with contradictory
/// bodies -- 0x30 with a TextSource member, and 0xC0 with Text and Appearance --
/// and two different structs expand it: FTextImpl::FTextData in Basic.hpp and
/// SDK::FTextData in mucha_structs.hpp. The later definition wins for both, so
/// the SDK does not compile as dumped.
///
/// Including Assertions.inl here, wrapped in namespace SDK because its asserts
/// use unqualified type names, makes every later include a no-op (it has
/// #pragma once). The redefinition below then survives and picks the right
/// layout per type, so both keep a real check rather than being suppressed.
#define SDK_NAMESPACE_START namespace SDK {
#define SDK_NAMESPACE_END }
#include "Assertions.inl"
#undef SDK_NAMESPACE_START
#undef SDK_NAMESPACE_END

// Dumper-7 emits the same macro for FTextImpl::FTextData and SDK::FTextData.
// Check both layouts using their actual members, without suppressing assertions.
template <class T> constexpr bool check_text_data() {
    static_assert(alignof(T) == 8);
    if constexpr (requires(T value) { value.TextSource; }) {
        static_assert(sizeof(T) == 0x30);
        static_assert(offsetof(T, TextSource) == 0x20);
    } else {
        static_assert(sizeof(T) == 0xC0);
        static_assert(offsetof(T, Text) == 0);
        static_assert(offsetof(T, Appearance) == 0x10);
    }
    return true;
}

#undef DUMPER7_ASSERTS_FTextData
#define DUMPER7_ASSERTS_FTextData static_assert(check_text_data<FTextData>())
