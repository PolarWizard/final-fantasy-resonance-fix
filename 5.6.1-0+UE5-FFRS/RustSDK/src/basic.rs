//! Reflection types with offsets Dumper-7 found for this game (the C++ SDK's Basic.hpp).

use crate::rt;

/// FFieldClass
#[repr(C, align(0x08))]
pub struct FFieldClass {
    pub Name: rt::FName, // 0x0000(0x0008)
    pub Id: u64, // 0x0008(0x0008)
    pub CastFlags: rt::EClassCastFlags, // 0x0010(0x0008)
    pub ClassFlags: rt::EClassFlags, // 0x0018(0x0004)
    pub Pad_1C: [u8; 0x4],
    pub SuperClass: *mut FFieldClass, // 0x0020(0x0008)
}
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FFieldClass>() == 0x0028);
    assert!(::core::mem::offset_of!(FFieldClass, Name) == 0x0000);
    assert!(::core::mem::offset_of!(FFieldClass, Id) == 0x0008);
    assert!(::core::mem::offset_of!(FFieldClass, CastFlags) == 0x0010);
    assert!(::core::mem::offset_of!(FFieldClass, ClassFlags) == 0x0018);
    assert!(::core::mem::offset_of!(FFieldClass, SuperClass) == 0x0020);
};

/// FFieldVariant
#[repr(C, align(0x08))]
pub struct FFieldVariant {
    pub Container: *mut ::core::ffi::c_void, // 0x0000(0x0008)
}
impl FFieldVariant {
    pub const UObjectMask: u64 = 0x1;
}
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FFieldVariant>() == 0x0008);
    assert!(::core::mem::offset_of!(FFieldVariant, Container) == 0x0000);
};

/// FField
#[repr(C, align(0x08))]
pub struct FField {
    pub VTable: *mut ::core::ffi::c_void, // 0x0000(0x0008)
    pub ClassPrivate: *mut FFieldClass, // 0x0008(0x0008)
    pub Owner: FFieldVariant, // 0x0010(0x0008)
    pub Next: *mut FField, // 0x0018(0x0008)
    pub Name: rt::FName, // 0x0020(0x0008)
    pub ObjFlags: rt::EObjectFlags, // 0x0028(0x0004)
}
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FField>() == 0x0030);
    assert!(::core::mem::offset_of!(FField, VTable) == 0x0000);
    assert!(::core::mem::offset_of!(FField, ClassPrivate) == 0x0008);
    assert!(::core::mem::offset_of!(FField, Owner) == 0x0010);
    assert!(::core::mem::offset_of!(FField, Next) == 0x0018);
    assert!(::core::mem::offset_of!(FField, Name) == 0x0020);
    assert!(::core::mem::offset_of!(FField, ObjFlags) == 0x0028);
};

/// FProperty
#[repr(C, align(0x08))]
pub struct FProperty {
    pub Super: FField,
    pub ArrayDim: i32, // 0x0030(0x0004)
    pub ElementSize: i32, // 0x0034(0x0004)
    pub PropertyFlags: rt::EPropertyFlags, // 0x0038(0x0008)
    pub Pad_40: [u8; 0x4],
    pub Offset: i32, // 0x0044(0x0004)
    pub Pad_48: [u8; 0x28],
}
rt::inherit!(FProperty => FField);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FProperty>() == 0x0070);
    assert!(::core::mem::offset_of!(FProperty, ArrayDim) == 0x0030);
    assert!(::core::mem::offset_of!(FProperty, ElementSize) == 0x0034);
    assert!(::core::mem::offset_of!(FProperty, PropertyFlags) == 0x0038);
    assert!(::core::mem::offset_of!(FProperty, Offset) == 0x0044);
};

/// FByteProperty
#[repr(C, align(0x08))]
pub struct FByteProperty {
    pub Super: FProperty,
    pub Enum: *mut crate::CoreUObject::UEnum, // 0x0070(0x0008)
}
rt::inherit!(FByteProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FByteProperty>() == 0x0078);
    assert!(::core::mem::offset_of!(FByteProperty, Enum) == 0x0070);
};

/// FBoolProperty
#[repr(C, align(0x08))]
pub struct FBoolProperty {
    pub Super: FProperty,
    pub FieldSize: u8, // 0x0070(0x0001)
    pub ByteOffset: u8, // 0x0071(0x0001)
    pub ByteMask: u8, // 0x0072(0x0001)
    pub FieldMask: u8, // 0x0073(0x0001)
}
rt::inherit!(FBoolProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FBoolProperty>() == 0x0078);
    assert!(::core::mem::offset_of!(FBoolProperty, FieldSize) == 0x0070);
    assert!(::core::mem::offset_of!(FBoolProperty, ByteOffset) == 0x0071);
    assert!(::core::mem::offset_of!(FBoolProperty, ByteMask) == 0x0072);
    assert!(::core::mem::offset_of!(FBoolProperty, FieldMask) == 0x0073);
};

/// FObjectPropertyBase
#[repr(C, align(0x08))]
pub struct FObjectPropertyBase {
    pub Super: FProperty,
    pub PropertyClass: *mut crate::CoreUObject::UClass, // 0x0070(0x0008)
}
rt::inherit!(FObjectPropertyBase => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FObjectPropertyBase>() == 0x0078);
    assert!(::core::mem::offset_of!(FObjectPropertyBase, PropertyClass) == 0x0070);
};

/// FClassProperty
#[repr(C, align(0x08))]
pub struct FClassProperty {
    pub Super: FObjectPropertyBase,
    pub MetaClass: *mut crate::CoreUObject::UClass, // 0x0078(0x0008)
}
rt::inherit!(FClassProperty => FObjectPropertyBase);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FClassProperty>() == 0x0080);
    assert!(::core::mem::offset_of!(FClassProperty, MetaClass) == 0x0078);
};

/// FStructProperty
#[repr(C, align(0x08))]
pub struct FStructProperty {
    pub Super: FProperty,
    pub Struct: *mut crate::CoreUObject::UStruct, // 0x0070(0x0008)
}
rt::inherit!(FStructProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FStructProperty>() == 0x0078);
    assert!(::core::mem::offset_of!(FStructProperty, Struct) == 0x0070);
};

/// FArrayProperty
#[repr(C, align(0x08))]
pub struct FArrayProperty {
    pub Super: FProperty,
    pub Pad_70: [u8; 0x8],
    pub InnerProperty: *mut FProperty, // 0x0078(0x0008)
}
rt::inherit!(FArrayProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FArrayProperty>() == 0x0080);
    assert!(::core::mem::offset_of!(FArrayProperty, InnerProperty) == 0x0078);
};

/// FDelegateProperty
#[repr(C, align(0x08))]
pub struct FDelegateProperty {
    pub Super: FProperty,
    pub SignatureFunction: *mut crate::CoreUObject::UFunction, // 0x0070(0x0008)
}
rt::inherit!(FDelegateProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FDelegateProperty>() == 0x0078);
    assert!(::core::mem::offset_of!(FDelegateProperty, SignatureFunction) == 0x0070);
};

/// FMapProperty
#[repr(C, align(0x08))]
pub struct FMapProperty {
    pub Super: FProperty,
    pub KeyProperty: *mut FProperty, // 0x0070(0x0008)
    pub ValueProperty: *mut FProperty, // 0x0078(0x0008)
}
rt::inherit!(FMapProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FMapProperty>() == 0x0080);
    assert!(::core::mem::offset_of!(FMapProperty, KeyProperty) == 0x0070);
    assert!(::core::mem::offset_of!(FMapProperty, ValueProperty) == 0x0078);
};

/// FSetProperty
#[repr(C, align(0x08))]
pub struct FSetProperty {
    pub Super: FProperty,
    pub ElementProperty: *mut FProperty, // 0x0070(0x0008)
}
rt::inherit!(FSetProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FSetProperty>() == 0x0078);
    assert!(::core::mem::offset_of!(FSetProperty, ElementProperty) == 0x0070);
};

/// FEnumProperty
#[repr(C, align(0x08))]
pub struct FEnumProperty {
    pub Super: FProperty,
    pub UnderlayingProperty: *mut FProperty, // 0x0070(0x0008)
    pub Enum: *mut crate::CoreUObject::UEnum, // 0x0078(0x0008)
}
rt::inherit!(FEnumProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FEnumProperty>() == 0x0080);
    assert!(::core::mem::offset_of!(FEnumProperty, UnderlayingProperty) == 0x0070);
    assert!(::core::mem::offset_of!(FEnumProperty, Enum) == 0x0078);
};

/// FFieldPathProperty
#[repr(C, align(0x08))]
pub struct FFieldPathProperty {
    pub Super: FProperty,
    pub FieldClass: *mut FFieldClass, // 0x0070(0x0008)
}
rt::inherit!(FFieldPathProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FFieldPathProperty>() == 0x0078);
    assert!(::core::mem::offset_of!(FFieldPathProperty, FieldClass) == 0x0070);
};

/// FOptionalProperty
#[repr(C, align(0x08))]
pub struct FOptionalProperty {
    pub Super: FProperty,
    pub ValueProperty: *mut FProperty, // 0x0070(0x0008)
}
rt::inherit!(FOptionalProperty => FProperty);
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FOptionalProperty>() == 0x0078);
    assert!(::core::mem::offset_of!(FOptionalProperty, ValueProperty) == 0x0070);
};

/// FFieldPath
#[repr(C, align(0x08))]
#[derive(Clone, Copy)]
pub struct FFieldPath {
    pub ResolvedField: *mut FField, // 0x0000(0x0008)
    pub ResolvedOwner: rt::TWeakObjectPtr<crate::CoreUObject::UStruct>, // 0x0008(0x0008)
    pub Path: rt::TArray<rt::FName>, // 0x0010(0x0010)
}
#[cfg(feature = "layout-checks")]
const _: () = {
    assert!(::core::mem::size_of::<FFieldPath>() == 0x0020);
    assert!(::core::mem::offset_of!(FFieldPath, ResolvedField) == 0x0000);
    assert!(::core::mem::offset_of!(FFieldPath, ResolvedOwner) == 0x0008);
    assert!(::core::mem::offset_of!(FFieldPath, Path) == 0x0010);
};
