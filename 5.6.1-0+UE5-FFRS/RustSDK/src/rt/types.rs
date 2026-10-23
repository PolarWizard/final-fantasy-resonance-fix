//! Pointer wrappers, delegates, field paths, optionals, fixed-size enums and engine flag enums.

use core::ffi::c_void;
use core::marker::PhantomData;
use core::mem::{size_of, MaybeUninit};
use core::ptr::null_mut;

use super::{FName, FString, GObjects, TArray};
use crate::config::{self, UClass, UObject};

/// Implemented by every UE enum (via `ue_enum!`).
pub trait UEnum: Copy {
    fn from_u64(value: u64) -> Self;
    fn to_u64(self) -> u64;

    /// The C++ SDK's `Flags & Other`: whether all bits of `Other` are set.
    fn HasFlags(self, Other: Self) -> bool {
        self.to_u64() & Other.to_u64() == Other.to_u64()
    }
}

/// Integer types a `TFixedSizeEnum` can be stored as.
pub trait EnumStorage: Copy {
    fn to_u64(self) -> u64;
    fn from_u64(value: u64) -> Self;
}

macro_rules! impl_enum_storage {
    ($($t:ty),*) => {
        $(impl EnumStorage for $t {
            #[inline] fn to_u64(self) -> u64 { self as u64 }
            #[inline] fn from_u64(value: u64) -> Self { value as $t }
        })*
    };
}
impl_enum_storage!(u8, u16, u32, u64, i8, i16, i32, i64);

/// An enum stored with a different size than its own (e.g. `TEnumAsByte` of a 4-byte enum).
#[repr(transparent)]
pub struct TFixedSizeEnum<E, T> {
    pub Value: T,
    _enum: PhantomData<E>,
}

impl<E, T: Copy> Clone for TFixedSizeEnum<E, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<E, T: Copy> Copy for TFixedSizeEnum<E, T> {}

impl<E: UEnum, T: EnumStorage> TFixedSizeEnum<E, T> {
    pub fn new(Value: E) -> Self {
        Self { Value: T::from_u64(Value.to_u64()), _enum: PhantomData }
    }

    pub fn Get(&self) -> E {
        E::from_u64(self.Value.to_u64())
    }

    pub fn Set(&mut self, Value: E) {
        self.Value = T::from_u64(Value.to_u64());
    }
}

/// `TSubclassOf<T>`: a UClass pointer that should describe `T` or a subclass.
#[repr(C)]
pub struct TSubclassOf<T> {
    pub ClassPtr: *mut UClass,
    _type: PhantomData<*const T>,
}

impl<T> Clone for TSubclassOf<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TSubclassOf<T> {}

impl<T> PartialEq for TSubclassOf<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ClassPtr == other.ClassPtr
    }
}

impl<T> TSubclassOf<T> {
    pub fn new(Class: *mut UClass) -> Self {
        Self { ClassPtr: Class, _type: PhantomData }
    }

    pub fn Get(&self) -> *mut UClass {
        self.ClassPtr
    }

    /// The same class as a `TSubclassOf<U>`, e.g. to pass `TSubclassOf<UConsole>` where `TSubclassOf<UObject>`
    /// is expected. Unlike the C++ SDK's implicit conversion, `U` being a base of `T` isn't checked.
    pub fn Cast<U>(self) -> TSubclassOf<U> {
        TSubclassOf::new(self.ClassPtr)
    }
}

/// `TSubclassOf<T>` from a class pointer (the C++ SDK's `TSubclassOf(UClass*)`).
impl<T> From<*mut UClass> for TSubclassOf<T> {
    fn from(Class: *mut UClass) -> Self {
        Self::new(Class)
    }
}

/// The class pointer of a `TSubclassOf<T>` (the C++ SDK's `operator UClass*`).
impl<T> From<TSubclassOf<T>> for *mut UClass {
    fn from(Class: TSubclassOf<T>) -> Self {
        Class.ClassPtr
    }
}

/// `FWeakObjectPtr`
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct FWeakObjectPtr {
    pub ObjectIndex: i32,
    pub ObjectSerialNumber: i32,
}

impl FWeakObjectPtr {
    pub fn IsValid(&self) -> bool {
        self.ObjectIndex > 0
    }

    /// The object, or null. Like the C++ SDK, the serial number isn't checked.
    pub fn Get(&self) -> *mut UObject {
        if !self.IsValid() {
            return null_mut();
        }
        GObjects.GetByIndex(self.ObjectIndex)
    }
}

impl PartialEq<*mut UObject> for FWeakObjectPtr {
    fn eq(&self, other: &*mut UObject) -> bool {
        self.Get() == *other
    }
}

/// `TWeakObjectPtr<T>`
#[repr(C)]
pub struct TWeakObjectPtr<T> {
    pub Weak: FWeakObjectPtr,
    _type: PhantomData<*const T>,
}

impl<T> Clone for TWeakObjectPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TWeakObjectPtr<T> {}

impl<T> PartialEq for TWeakObjectPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.Weak == other.Weak
    }
}

impl<T> TWeakObjectPtr<T> {
    pub fn IsValid(&self) -> bool {
        self.Weak.IsValid()
    }

    pub fn Get(&self) -> *mut T {
        self.Weak.Get().cast()
    }
}

/// `FUniqueObjectGuid`
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct FUniqueObjectGuid {
    pub A: u32,
    pub B: u32,
    pub C: u32,
    pub D: u32,
}

/// `TPersistentObjectPtr<Id>`: base of lazy and soft pointers.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TPersistentObjectPtr<Id> {
    pub WeakPtr: FWeakObjectPtr,
    pub TagAtLastTest: [i32; config::WEAK_POINTER_HAS_TAG as usize],
    pub ObjectID: Id,
}

impl<Id> TPersistentObjectPtr<Id> {
    /// The object if it's loaded, or null.
    pub fn Get(&self) -> *mut UObject {
        self.WeakPtr.Get()
    }
}

/// `TLazyObjectPtr<T>`
#[repr(C)]
pub struct TLazyObjectPtr<T> {
    pub Ptr: TPersistentObjectPtr<FUniqueObjectGuid>,
    _type: PhantomData<*const T>,
}

impl<T> Clone for TLazyObjectPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TLazyObjectPtr<T> {}

impl<T> TLazyObjectPtr<T> {
    pub fn Get(&self) -> *mut T {
        self.Ptr.Get().cast()
    }
}

/// Stand-in for `FSoftObjectPath` on games that don't reflect it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FSoftObjectPathFallback {
    pub AssetLongPathname: FString,
}

/// `TSoftObjectPtr<T>` (also used for `TSoftClassPtr<T>`)
#[repr(C)]
pub struct TSoftObjectPtr<T> {
    pub Ptr: TPersistentObjectPtr<config::FSoftObjectPath>,
    _type: PhantomData<*const T>,
}

impl<T> Clone for TSoftObjectPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TSoftObjectPtr<T> {}

impl<T> TSoftObjectPtr<T> {
    /// The object if it's loaded, or null.
    pub fn Get(&self) -> *mut T {
        self.Ptr.Get().cast()
    }
}

/// `TSoftClassPtr<T>`
pub type TSoftClassPtr<T> = TSoftObjectPtr<T>;

/// `TEncryptedObjPtr<T>` (only used when encrypted object properties are enabled in Dumper-7).
#[repr(C)]
pub struct TEncryptedObjPtr<T> {
    pub Value: u64,
    _type: PhantomData<*const T>,
}

impl<T> Clone for TEncryptedObjPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TEncryptedObjPtr<T> {}

/// Same bit layout as the C++ SDK's `union { EncryptionIndex : 4; Object : 60; }`.
impl<T> TEncryptedObjPtr<T> {
    const OBJECT_MASK: u64 = (1 << 60) - 1;

    pub fn EncryptionIndex(&self) -> u8 {
        (self.Value & 0xF) as u8
    }

    pub fn Get(&self) -> *mut T {
        (self.Value & Self::OBJECT_MASK) as usize as *mut T
    }

    pub fn IsValid(&self) -> bool {
        self.Value & Self::OBJECT_MASK != 0
    }
}

impl<T> PartialEq for TEncryptedObjPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.Value & Self::OBJECT_MASK == other.Value & Self::OBJECT_MASK
    }
}

/// `FScriptInterface`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FScriptInterface {
    pub ObjectPointer: *mut UObject,
    pub InterfacePointer: *mut c_void,
}

impl FScriptInterface {
    pub fn GetObjectRef(&self) -> *mut UObject {
        self.ObjectPointer
    }

    pub fn GetInterfaceRef(&self) -> *mut c_void {
        self.InterfacePointer
    }
}

/// `TScriptInterface<T>`
#[repr(C)]
pub struct TScriptInterface<T> {
    pub Interface: FScriptInterface,
    _type: PhantomData<*const T>,
}

impl<T> Clone for TScriptInterface<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TScriptInterface<T> {}

impl<T> TScriptInterface<T> {
    pub fn GetObjectRef(&self) -> *mut UObject {
        self.Interface.ObjectPointer
    }

    pub fn GetInterfaceRef(&self) -> *mut T {
        self.Interface.InterfacePointer.cast()
    }
}

/// `FScriptDelegate`: an object and the name of the function to call on it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FScriptDelegate {
    pub Object: FWeakObjectPtr,
    pub FunctionName: FName,
}

/// `TDelegate<Signature>`. `Signature` is a `fn(...)` type that documents the bound function.
#[repr(C)]
pub struct TDelegate<Signature = ()> {
    pub BoundFunction: FScriptDelegate,
    pub Rest: [u8; config::DELEGATE_SIZE - size_of::<FScriptDelegate>()],
    _signature: PhantomData<Signature>,
}

impl<Signature> Clone for TDelegate<Signature> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Signature> Copy for TDelegate<Signature> {}

/// `TMulticastInlineDelegate<Signature>`. `Signature` is a `fn(...)` type that documents the bound functions.
#[repr(C)]
pub struct TMulticastInlineDelegate<Signature = ()> {
    pub Rest: [u8; config::MULTICAST_INLINE_DELEGATE_SIZE - size_of::<TArray<FScriptDelegate>>()],
    pub InvocationList: TArray<FScriptDelegate>,
    _signature: PhantomData<Signature>,
}

impl<Signature> Clone for TMulticastInlineDelegate<Signature> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Signature> Copy for TMulticastInlineDelegate<Signature> {}

/// `TFieldPath<PropertyType>`: an `FFieldPath` (generated in `basic.rs`, its layout depends on the game)
/// to a property of type `PropertyType`.
#[repr(transparent)]
pub struct TFieldPath<PropertyType> {
    pub Path: crate::basic::FFieldPath,
    _type: PhantomData<*const PropertyType>,
}

impl<PropertyType> Clone for TFieldPath<PropertyType> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<PropertyType> Copy for TFieldPath<PropertyType> {}

impl<PropertyType> TFieldPath<PropertyType> {
    /// The resolved property, or null if the path hasn't been resolved.
    pub fn Get(&self) -> *mut PropertyType {
        self.Path.ResolvedField.cast()
    }
}

impl<PropertyType> core::ops::Deref for TFieldPath<PropertyType> {
    type Target = crate::basic::FFieldPath;
    fn deref(&self) -> &crate::basic::FFieldPath {
        &self.Path
    }
}

/// `TOptional<T>` with its `bIsSet` flag.
#[repr(C)]
pub struct TOptional<T> {
    pub Value: MaybeUninit<T>,
    pub bIsSet: bool,
}

impl<T: Copy> Clone for TOptional<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Copy> Copy for TOptional<T> {}

impl<T> TOptional<T> {
    pub fn IsSet(&self) -> bool {
        self.bIsSet
    }

    /// The value, if it's set.
    pub fn GetValueRef(&self) -> Option<&T> {
        if self.bIsSet {
            Some(unsafe { self.Value.assume_init_ref() })
        } else {
            None
        }
    }
}

/// `TOptional<T>` for types that mark "unset" inside the value itself (no flag).
#[repr(C)]
pub struct TOptionalNoFlag<T> {
    pub Value: MaybeUninit<T>,
}

impl<T: Copy> Clone for TOptionalNoFlag<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Copy> Copy for TOptionalNoFlag<T> {}

impl<T> TOptionalNoFlag<T> {
    /// Whether any byte of the value is non-zero (an all-zero value counts as unset, like the C++ SDK).
    pub fn IsSet(&self) -> bool {
        let bytes = unsafe { core::slice::from_raw_parts(self.Value.as_ptr().cast::<u8>(), size_of::<T>()) };
        bytes.iter().any(|&b| b != 0)
    }

    pub fn GetValueRef(&self) -> Option<&T> {
        if self.IsSet() {
            Some(unsafe { self.Value.assume_init_ref() })
        } else {
            None
        }
    }
}

/// `FStructBaseChain`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FStructBaseChain {
    pub StructBaseChainArray: *mut *mut FStructBaseChain,
    pub NumStructBasesInChainMinusOne: i32,
}

crate::rt::ue_enum! {
    /// `EObjectFlags`
    EObjectFlags: u32 {
        NoFlags = 0x00000000u64,
        Public = 0x00000001u64,
        Standalone = 0x00000002u64,
        MarkAsNative = 0x00000004u64,
        Transactional = 0x00000008u64,
        ClassDefaultObject = 0x00000010u64,
        ArchetypeObject = 0x00000020u64,
        Transient = 0x00000040u64,
        MarkAsRootSet = 0x00000080u64,
        TagGarbageTemp = 0x00000100u64,
        NeedInitialization = 0x00000200u64,
        NeedLoad = 0x00000400u64,
        KeepForCooker = 0x00000800u64,
        NeedPostLoad = 0x00001000u64,
        NeedPostLoadSubobjects = 0x00002000u64,
        NewerVersionExists = 0x00004000u64,
        BeginDestroyed = 0x00008000u64,
        FinishDestroyed = 0x00010000u64,
        BeingRegenerated = 0x00020000u64,
        DefaultSubObject = 0x00040000u64,
        WasLoaded = 0x00080000u64,
        TextExportTransient = 0x00100000u64,
        LoadCompleted = 0x00200000u64,
        InheritableComponentTemplate = 0x00400000u64,
        DuplicateTransient = 0x00800000u64,
        StrongRefOnFrame = 0x01000000u64,
        NonPIEDuplicateTransient = 0x02000000u64,
        Dynamic = 0x04000000u64,
        WillBeLoaded = 0x08000000u64,
    }
}

crate::rt::ue_enum! {
    /// `EFunctionFlags`
    EFunctionFlags: u32 {
        None = 0x00000000u64,
        Final = 0x00000001u64,
        RequiredAPI = 0x00000002u64,
        BlueprintAuthorityOnly = 0x00000004u64,
        BlueprintCosmetic = 0x00000008u64,
        Net = 0x00000040u64,
        NetReliable = 0x00000080u64,
        NetRequest = 0x00000100u64,
        Exec = 0x00000200u64,
        Native = 0x00000400u64,
        Event = 0x00000800u64,
        NetResponse = 0x00001000u64,
        Static = 0x00002000u64,
        NetMulticast = 0x00004000u64,
        UbergraphFunction = 0x00008000u64,
        MulticastDelegate = 0x00010000u64,
        Public = 0x00020000u64,
        Private = 0x00040000u64,
        Protected = 0x00080000u64,
        Delegate = 0x00100000u64,
        NetServer = 0x00200000u64,
        HasOutParms = 0x00400000u64,
        HasDefaults = 0x00800000u64,
        NetClient = 0x01000000u64,
        DLLImport = 0x02000000u64,
        BlueprintCallable = 0x04000000u64,
        BlueprintEvent = 0x08000000u64,
        BlueprintPure = 0x10000000u64,
        EditorOnly = 0x20000000u64,
        Const = 0x40000000u64,
        NetValidate = 0x80000000u64,
    }
}

crate::rt::ue_enum! {
    /// `EClassFlags`
    EClassFlags: u32 {
        None = 0x00000000u64,
        Abstract = 0x00000001u64,
        DefaultConfig = 0x00000002u64,
        Config = 0x00000004u64,
        Transient = 0x00000008u64,
        Parsed = 0x00000010u64,
        MatchedSerializers = 0x00000020u64,
        ProjectUserConfig = 0x00000040u64,
        Native = 0x00000080u64,
        NoExport = 0x00000100u64,
        NotPlaceable = 0x00000200u64,
        PerObjectConfig = 0x00000400u64,
        ReplicationDataIsSetUp = 0x00000800u64,
        EditInlineNew = 0x00001000u64,
        CollapseCategories = 0x00002000u64,
        Interface = 0x00004000u64,
        CustomConstructor = 0x00008000u64,
        Const = 0x00010000u64,
        LayoutChanging = 0x00020000u64,
        CompiledFromBlueprint = 0x00040000u64,
        MinimalAPI = 0x00080000u64,
        RequiredAPI = 0x00100000u64,
        DefaultToInstanced = 0x00200000u64,
        TokenStreamAssembled = 0x00400000u64,
        HasInstancedReference = 0x00800000u64,
        Hidden = 0x01000000u64,
        Deprecated = 0x02000000u64,
        HideDropDown = 0x04000000u64,
        GlobalUserConfig = 0x08000000u64,
        Intrinsic = 0x10000000u64,
        Constructed = 0x20000000u64,
        ConfigDoNotCheckDefaults = 0x40000000u64,
        NewerVersionExists = 0x80000000u64,
    }
}

crate::rt::ue_enum! {
    /// `EClassCastFlags`
    EClassCastFlags: u64 {
        None = 0x0000000000000000u64,
        Field = 0x0000000000000001u64,
        Int8Property = 0x0000000000000002u64,
        Enum = 0x0000000000000004u64,
        Struct = 0x0000000000000008u64,
        ScriptStruct = 0x0000000000000010u64,
        Class = 0x0000000000000020u64,
        ByteProperty = 0x0000000000000040u64,
        IntProperty = 0x0000000000000080u64,
        FloatProperty = 0x0000000000000100u64,
        UInt64Property = 0x0000000000000200u64,
        ClassProperty = 0x0000000000000400u64,
        UInt32Property = 0x0000000000000800u64,
        InterfaceProperty = 0x0000000000001000u64,
        NameProperty = 0x0000000000002000u64,
        StrProperty = 0x0000000000004000u64,
        Property = 0x0000000000008000u64,
        ObjectProperty = 0x0000000000010000u64,
        BoolProperty = 0x0000000000020000u64,
        UInt16Property = 0x0000000000040000u64,
        Function = 0x0000000000080000u64,
        StructProperty = 0x0000000000100000u64,
        ArrayProperty = 0x0000000000200000u64,
        Int64Property = 0x0000000000400000u64,
        DelegateProperty = 0x0000000000800000u64,
        NumericProperty = 0x0000000001000000u64,
        MulticastDelegateProperty = 0x0000000002000000u64,
        ObjectPropertyBase = 0x0000000004000000u64,
        WeakObjectProperty = 0x0000000008000000u64,
        LazyObjectProperty = 0x0000000010000000u64,
        SoftObjectProperty = 0x0000000020000000u64,
        TextProperty = 0x0000000040000000u64,
        Int16Property = 0x0000000080000000u64,
        DoubleProperty = 0x0000000100000000u64,
        SoftClassProperty = 0x0000000200000000u64,
        Package = 0x0000000400000000u64,
        Level = 0x0000000800000000u64,
        Actor = 0x0000001000000000u64,
        PlayerController = 0x0000002000000000u64,
        Pawn = 0x0000004000000000u64,
        SceneComponent = 0x0000008000000000u64,
        PrimitiveComponent = 0x0000010000000000u64,
        SkinnedMeshComponent = 0x0000020000000000u64,
        SkeletalMeshComponent = 0x0000040000000000u64,
        Blueprint = 0x0000080000000000u64,
        DelegateFunction = 0x0000100000000000u64,
        StaticMeshComponent = 0x0000200000000000u64,
        MapProperty = 0x0000400000000000u64,
        SetProperty = 0x0000800000000000u64,
        EnumProperty = 0x0001000000000000u64,
        SparseDelegateFunction = 0x0002000000000000u64,
        MulticastInlineDelegateProperty = 0x0004000000000000u64,
        MulticastSparseDelegateProperty = 0x0008000000000000u64,
        FieldPathProperty = 0x0010000000000000u64,
        LargeWorldCoordinatesRealProperty = 0x0080000000000000u64,
        OptionalProperty = 0x0100000000000000u64,
        VValueProperty = 0x0200000000000000u64,
        VerseVMClass = 0x0400000000000000u64,
        VRestValueProperty = 0x0800000000000000u64,
        Utf8StrProperty = 0x1000000000000000u64,
        AnsiStrProperty = 0x2000000000000000u64,
        VCellProperty = 0x4000000000000000u64,
    }
}

crate::rt::ue_enum! {
    /// `EPropertyFlags`
    EPropertyFlags: u64 {
        None = 0x0000000000000000u64,
        Edit = 0x0000000000000001u64,
        ConstParm = 0x0000000000000002u64,
        BlueprintVisible = 0x0000000000000004u64,
        ExportObject = 0x0000000000000008u64,
        BlueprintReadOnly = 0x0000000000000010u64,
        Net = 0x0000000000000020u64,
        EditFixedSize = 0x0000000000000040u64,
        Parm = 0x0000000000000080u64,
        OutParm = 0x0000000000000100u64,
        ZeroConstructor = 0x0000000000000200u64,
        ReturnParm = 0x0000000000000400u64,
        DisableEditOnTemplate = 0x0000000000000800u64,
        Transient = 0x0000000000002000u64,
        Config = 0x0000000000004000u64,
        DisableEditOnInstance = 0x0000000000010000u64,
        EditConst = 0x0000000000020000u64,
        GlobalConfig = 0x0000000000040000u64,
        InstancedReference = 0x0000000000080000u64,
        DuplicateTransient = 0x0000000000200000u64,
        SubobjectReference = 0x0000000000400000u64,
        SaveGame = 0x0000000001000000u64,
        NoClear = 0x0000000002000000u64,
        ReferenceParm = 0x0000000008000000u64,
        BlueprintAssignable = 0x0000000010000000u64,
        Deprecated = 0x0000000020000000u64,
        IsPlainOldData = 0x0000000040000000u64,
        RepSkip = 0x0000000080000000u64,
        RepNotify = 0x0000000100000000u64,
        Interp = 0x0000000200000000u64,
        NonTransactional = 0x0000000400000000u64,
        EditorOnly = 0x0000000800000000u64,
        NoDestructor = 0x0000001000000000u64,
        AutoWeak = 0x0000004000000000u64,
        ContainsInstancedReference = 0x0000008000000000u64,
        AssetRegistrySearchable = 0x0000010000000000u64,
        SimpleDisplay = 0x0000020000000000u64,
        AdvancedDisplay = 0x0000040000000000u64,
        Protected = 0x0000080000000000u64,
        BlueprintCallable = 0x0000100000000000u64,
        BlueprintAuthorityOnly = 0x0000200000000000u64,
        TextExportTransient = 0x0000400000000000u64,
        NonPIEDuplicateTransient = 0x0000800000000000u64,
        ExposeOnSpawn = 0x0001000000000000u64,
        PersistentInstance = 0x0002000000000000u64,
        UObjectWrapper = 0x0004000000000000u64,
        HasGetValueTypeHash = 0x0008000000000000u64,
        NativeAccessSpecifierPublic = 0x0010000000000000u64,
        NativeAccessSpecifierProtected = 0x0020000000000000u64,
        NativeAccessSpecifierPrivate = 0x0040000000000000u64,
        SkipSerialization = 0x0080000000000000u64,
        TObjectPtr = 0x0100000000000000u64,
        ExperimentalOverridableLogic = 0x0200000000000000u64,
        ExperimentalAlwaysOverriden = 0x0400000000000000u64,
        ExperimentalNeverOverriden = 0x0800000000000000u64,
        AllowSelfReference = 0x1000000000000000u64,
    }
}
