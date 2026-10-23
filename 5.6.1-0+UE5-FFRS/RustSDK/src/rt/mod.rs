//! Hand-written runtime that ships with every Dumper-7 Rust SDK. Its API mirrors the C++ SDK's
//! (`UObject::FindObject`, `GetName`, `IsA`, `UWorld::GetWorld`, `TArray::Num`, ...).
//!
//! Every SDK type mirrors memory owned by the game. Turning a raw game pointer into a
//! reference (`unsafe { &mut *ptr }`) is the one unsafe step: only do it on the game
//! thread and only while the object is alive.
//!
//! UFunction wrappers take `&mut self`. That tells the compiler the game may change the
//! object during the call, so it never reuses a value it read before the call.
//!
//! Containers and strings never allocate or free engine memory. Memory the engine hands back
//! (e.g. an `FString` out-parameter) is leaked, exactly like the C++ SDK.

mod containers;
mod ffi;
mod math;
mod object;
mod strings;
mod types;

pub use containers::*;
pub use ffi::*;
pub use object::*;
pub use strings::*;
pub use types::*;

/// Inheritance: `inherit!(Child => Base)` makes `Child` deref (and deref-mut) to `Base`.
/// `inherit!(Child => Base, const)` omits `DerefMut`.
///
/// The base always sits at offset 0, so a pointer cast is valid for both a normal
/// `Super: Base` field and a `Super: TailPadded<Base, N>` field.
macro_rules! inherit {
    ($child:ty => $base:ty) => {
        $crate::rt::inherit!($child => $base, const);

        impl ::core::ops::DerefMut for $child {
            #[inline(always)]
            fn deref_mut(&mut self) -> &mut $base {
                unsafe { &mut *(self as *mut Self).cast::<$base>() }
            }
        }
    };
    ($child:ty => $base:ty, const) => {
        impl ::core::ops::Deref for $child {
            type Target = $base;

            #[inline(always)]
            fn deref(&self) -> &$base {
                unsafe { &*(self as *const Self).cast::<$base>() }
            }
        }
    };
}
pub(crate) use inherit;

/// Getter/setter pairs for UE bitfield bools: `Field[ByteIndex] & Mask => getter, setter;`
macro_rules! bitfields {
    ($t:ty { $($field:ident [$idx:literal] & $mask:literal => $get:ident, $set:ident;)* }) => {
        impl $t {
            $(
                #[inline]
                pub fn $get(&self) -> bool {
                    self.$field[$idx] & $mask != 0
                }

                #[inline]
                pub fn $set(&mut self, value: bool) {
                    if value {
                        self.$field[$idx] |= $mask;
                    } else {
                        self.$field[$idx] &= !$mask;
                    }
                }
            )*
        }
    };
}
pub(crate) use bitfields;

/// UE enums are newtypes, not Rust enums: the game can store values the dump never saw,
/// and an unknown value in a Rust `enum` is undefined behaviour.
///
/// `|`, `&`, `^`, `!` work bitwise. `rt::UEnum::HasFlags` is the C++ SDK's `Flags & Other`.
macro_rules! ue_enum {
    ($(#[$meta:meta])* $name:ident : $repr:ty { $($variant:ident = $value:literal),* $(,)? }) => {
        $(#[$meta])*
        #[repr(transparent)]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(pub $repr);

        impl $name {
            $(pub const $variant: Self = Self($value as $repr);)*

            #[doc(hidden)]
            pub const __VARIANTS: &'static [(&'static str, $repr)] = &[$((stringify!($variant), $value as $repr)),*];
        }

        impl $crate::rt::UEnum for $name {
            #[inline] fn from_u64(value: u64) -> Self { Self(value as $repr) }
            #[inline] fn to_u64(self) -> u64 { self.0 as u64 }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                for (name, value) in Self::__VARIANTS {
                    if *value == self.0 {
                        return f.write_str(name);
                    }
                }
                ::core::write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl ::core::ops::BitOr for $name {
            type Output = Self;
            #[inline] fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
        }
        impl ::core::ops::BitAnd for $name {
            type Output = Self;
            #[inline] fn bitand(self, rhs: Self) -> Self { Self(self.0 & rhs.0) }
        }
        impl ::core::ops::BitXor for $name {
            type Output = Self;
            #[inline] fn bitxor(self, rhs: Self) -> Self { Self(self.0 ^ rhs.0) }
        }
        impl ::core::ops::Not for $name {
            type Output = Self;
            #[inline] fn not(self) -> Self { Self(!self.0) }
        }
        impl ::core::ops::BitOrAssign for $name {
            #[inline] fn bitor_assign(&mut self, rhs: Self) { self.0 |= rhs.0 }
        }
        impl ::core::ops::BitAndAssign for $name {
            #[inline] fn bitand_assign(&mut self, rhs: Self) { self.0 &= rhs.0 }
        }
    };
}
pub(crate) use ue_enum;

/// `StaticClass()`, `StaticName()` and `GetDefaultObj()` for a UClass type, found by name and cached.
/// `uclass!(T, "Name")`, or `uclass!(T, full "Class Package.Name", "Name")` when the name isn't unique.
macro_rules! uclass {
    ($t:ty, full $full_name:literal, $name:literal) => {
        $crate::rt::uclass!(@impl $t, $full_name, true, $name);
    };
    ($t:ty, $name:literal) => {
        $crate::rt::uclass!(@impl $t, $name, false, $name);
    };
    (@impl $t:ty, $class_name:literal, $full:literal, $name:literal) => {
        impl $t {
            /// Used with `IsA` to check if an object is of a certain type. Null if the class isn't loaded.
            pub fn StaticClass() -> *mut $crate::config::UClass {
                static CACHE: $crate::rt::ClassCache = $crate::rt::ClassCache::new($class_name, $full);
                CACHE.get()
            }

            /// Used with `IsA` to check if an object is of a certain Blueprint class type.
            pub fn StaticName() -> $crate::rt::FName {
                static CACHE: $crate::rt::NameCache = $crate::rt::NameCache::new($name);
                CACHE.get()
            }

            /// Only use the default object to call "static" functions.
            pub fn GetDefaultObj() -> *mut Self {
                $crate::rt::default_object(Self::StaticClass()).cast()
            }
        }

        unsafe impl $crate::rt::UClassType for $t {
            fn static_class() -> *mut $crate::config::UClass {
                Self::StaticClass()
            }
        }
    };
}
pub(crate) use uclass;

/// `AsUObject()` for interface types (interfaces don't inherit from UObject in the SDK).
macro_rules! uinterface {
    ($t:ty) => {
        impl $t {
            /// UObject inheritance was removed from interfaces to avoid virtual inheritance in the SDK.
            pub fn AsUObject(&mut self) -> &mut $crate::config::UObject {
                unsafe { &mut *(self as *mut Self).cast::<$crate::config::UObject>() }
            }
        }
    };
}
pub(crate) use uinterface;

/// An `FString` over a UTF-16 copy of a string literal, like `L"..."` in the C++ SDK.
/// The text lives in static memory: only pass it where the game reads the string.
///
/// ```ignore
/// let name = UKismetStringLibrary::Conv_StringToName(&sdk::fstring!("DemoNetDriver"));
/// ```
#[macro_export]
macro_rules! fstring {
    ($text:literal) => {{
        const __LEN: usize = $crate::rt::utf16_len($text) + 1;
        static __TEXT: [u16; __LEN] = $crate::rt::utf16_encode::<__LEN>($text);
        $crate::rt::FString::from_raw(__TEXT.as_ptr().cast_mut(), __LEN as i32, __LEN as i32)
    }};
}
