// Cpp file used to generate `test.pdb`
#include <Windows.h>

#include <cstddef>
#include <cstdint>
#include <memory>

namespace resym_test {

struct PrimitiveTypesTest {
  // Bool
  bool b1;

  // Chars
  char c1;
  unsigned char c2;
  char8_t c3;
  char16_t c4;
  char32_t c5;
  wchar_t w1;

  // Integers
  unsigned short i1;
  short i2;
  unsigned int i3;
  int i4;
  unsigned long i5;
  long i6;
  unsigned __int64 i7;
  __int64 i8;
  unsigned long long i9;
  long long i10;

  // Floating points
  float f1;
  long float f2;
  double f3;
  long double f4;

  HRESULT hres;
};

struct ArrayTest {
  // Single dimension
  char array1[64];
  int array2[64];
  PrimitiveTypesTest array3[64];

  // Multiple dimensions
  char array4[1][2][3][4][5];
  int array5[1][2][3][4][5];
  PrimitiveTypesTest array6[1][2][3][4][5];
};

struct BitFieldsTest1 {
  uint32_t b1 : 1;   // Bit offset: 0
  uint32_t b2 : 1;   // Bit offset: 1
  uint32_t b3 : 30;  // Bit offset: 2
};

struct BitFieldsTest2 {
  // Will usually occupy 2 bytes:
  // 3 bits: value of b1
  // 5 bits: unused
  // 6 bits: value of b2
  // 2 bits: value of b3
  unsigned char b1 : 3;  // Bit offset: 0
  unsigned char : 0;     // start a new byte
  unsigned char b2 : 6;  // Bit offset: 0
  unsigned char b3 : 2;  // Bit offset: 6
};

union BitFieldsTest3 {
  uint32_t b1 : 1;   // Bit offset: 0
  uint32_t b2 : 1;   // Bit offset: 0
  uint32_t b3 : 30;  // Bit offset: 0
};

union BitFieldsTest4 {
  struct {
    uint16_t b1 : 1;   // Bit offset: 0
    uint16_t b2 : 5;   // Bit offset: 1
    uint16_t b3 : 10;  // Bit offset: 6
  };
};

struct BitFieldsTest5 {
  union {
    uint16_t b1 : 1;   // Bit offset: 0
    uint16_t b2 : 5;   // Bit offset: 0
    uint16_t b3 : 10;  // Bit offset: 0
  };
};

struct BitFieldsTest6 {
  // will usually occupy 2 bytes:
  // 3 bits: value of b1
  // 2 bits: unused
  // 6 bits: value of b2
  // 2 bits: value of b3
  // 3 bits: unused
  uint16_t b1 : 3, : 2, b2 : 6, b3 : 2;
};

struct BitFieldsTest7 {
  uint16_t b1 : 3;  // Bit offset: 0
  uint32_t b2 : 3;  // Bit offset: 0
};

union UnionTest {
  UnionTest() : u4() {}
  ~UnionTest() {}

  void* GetPtr() { return nullptr; }
  void* ConstMethod() const { return nullptr; }
  void* VolatileMethod() volatile { return nullptr; }
  void* ConstVolatileMethod() const volatile { return nullptr; }
  void (*ReturnFuncPointerMethod())(int) { return nullptr; }
  static int Magic() { return 42; }
  static int MagicVar1(...) { return 64; }
  static int MagicVar2(int arg...) { return 96; }

  uint8_t u1;
  uint16_t u2;
  uint32_t u3;
  uint64_t u4;
  static uint64_t su5;
};

struct StructTest {
  StructTest() : u1(), u2(), u3(), u4() {}
  ~StructTest() {}

  void* GetPtr() { return nullptr; }
  void* ConstMethod() const { return nullptr; }
  void* VolatileMethod() volatile { return nullptr; }
  void* ConstVolatileMethod() const volatile { return nullptr; }
  void (*ReturnFuncPointerMethod())(int) { return nullptr; }
  virtual int Virtual() { return 37; }
  static int Magic() { return 42; }
  static int MagicVar1(...) { return 64; }
  static int MagicVar2(int arg...) { return 96; }

  uint8_t u1;
  uint16_t u2;
  uint32_t u3;
  uint64_t u4;
  static uint64_t su5;
};

enum EnumTest1 {
  kEnumTest1Val1 = 0,
  kEnumTest1Val2,
  kEnumTest1Val3,
};

enum EnumTest2 : uint8_t {
  kEnumTest2Val1 = 0,
  kEnumTest2Val2,
  kEnumTest2Val3,
};

enum class EnumClassTest1 {
  kEnumClassTest1Val1 = 0,
  kEnumClassTest1Val2,
  kEnumClassTest1Val3,
};

enum class EnumClassTest2 : uint8_t {
  kEnumClassTest2Val1 = 0,
  kEnumClassTest2Val2,
  kEnumClassTest2Val3,
};

class PureVirtualClass {
  virtual int InterfaceVirtual() = 0;
};

class InterfaceImplClass : public PureVirtualClass {
  int InterfaceVirtual() override { return 13; }
};

class PureVirtualClassSpecialized : public PureVirtualClass {
  int OtherMethod() { return 2; }
};

class SpecializedInterfaceImplClass : public PureVirtualClassSpecialized {
  int InterfaceVirtual() override { return 13; }
};

class ClassWithRefsAndStaticsTest {
  int& iref{sint};
  const int& ciref{sint};
  int* iptr{};
  const int* ciptr{};
  bool& bref{sbool};
  const bool& cbref{sbool};
  bool* bptr{};
  const bool* cbptr{};

  static int sint;
  static bool sbool;
};
int ClassWithRefsAndStaticsTest::sint;
bool ClassWithRefsAndStaticsTest::sbool;

class ClassWithNestedDeclarationsTest {
  struct NestedStruct {
    int field;
  };

  class NestedClass {
    int field;
  };

  union NestedUnion {
    int field;
  };

  enum NestEnum { kHello };
};

union UnionWithNestedDeclarationsTest {
  struct NestedStruct {
    int field;
  };

  class NestedClass {
    int field;
  };

  union NestedUnion {
    int field;
  };

  enum NestEnum { kHello };
};

union UnionUnnamedUdtTest1 {
  struct {
    uint32_t i1;
    uint32_t i2;
  };
  PrimitiveTypesTest s1;
  uint64_t QuadPart;
  struct {
    uint32_t i11;
    uint32_t i22;
  };
};

struct StructUnnamedUdtTest1 {
  union {
    struct {
      uint32_t i1;  // +0x0
      uint32_t i2;  // +0x4
      union {
        uint32_t i3;  // +0x8
        uint32_t i4;  // +0x8
      };
    };
    uint32_t i5;  // +0x0
    struct {
      uint32_t i21;  // +0x0
      uint32_t i22;  // +0x4
      uint32_t i23;  // +0x8
    };
    PrimitiveTypesTest s1;

    uint64_t QuadPart;  // +0x0
  };
  uint64_t QuadPart2;  // +0x10
  uint64_t QuadPart3;  // +0x18
  union {
    unsigned long Reserved;  // +0x20
    struct {
      unsigned char Type;        // +0x20
      unsigned char Reserved1;   // +0x21
      unsigned short Reserved2;  // +0x22
    };
  };
  int32_t i6;  // +0x24
  int32_t i7;  // +0x28
  union {
    void* c1;  // +0x2C
    char c2;   // +0x2C
  };
  int32_t i8;  // +0x38
  int32_t i9;  // +0x3C
};
// TODO: Detect alginment issues

struct StructUnnamedUdtTest2 {
  UINT64 Before;
  union {
    struct {
      UINT64 u1;
      UINT64 u2;
    };
    struct {
      PUINT64 p1;
      PUINT64 p2;
    };
  };
  UINT64 Middle;
  union {
    UINT64 u3;
    PUINT64 p3;
  };
  UINT64 After;
};

struct StructUnnamedUdtTest3 {
  UINT64 Before;
  union {
    struct {
      UINT64 u1;
      UINT64 u2;
    };
    struct {
      PUINT64 p1;
      PUINT64 p2;
      PUINT64 p3;
      PUINT64 p4;
    };
    struct {
      PUINT64 p5;
      PUINT64 p6;
    };
  };
  UINT64 Middle;
  union {
    UINT64 u3;
    PUINT64 p7;
  };
  UINT64 After;
};

struct StructAccessTest {
  int public1;

 private:
  int private1;

 protected:
  int protected1;

 public:
  int public2;
};

class ClassAccessTest {
  int private1;

 public:
  int public1;

 private:
  int private2;

 protected:
  int protected1;
};

union UnionAccessTest {
  int public1;

 private:
  int private1;

 protected:
  int protected1;

 public:
  int public2;
};

struct BigOffsetsStruct {
  char a[65536];
  char b[65536];
};

struct NestedStructUnionRegression1 {
  /* 0x0000 */ struct _LIST_ENTRY TransactionListEntry;
  /* 0x0010 */ struct _CM_INTENT_LOCK* KCBLock;
  /* 0x0018 */ struct _CM_INTENT_LOCK* KeyLock;
  /* 0x0020 */ struct _LIST_ENTRY KCBListEntry;
  /* 0x0030 */ struct _CM_KEY_CONTROL_BLOCK* KeyControlBlock;
  /* 0x0038 */ struct _CM_TRANS* Transaction;
  /* 0x0040 */ unsigned long UoWState;
  /* 0x0044 */ enum UoWActionType ActionType;
  /* 0x0048 */ enum HSTORAGE_TYPE StorageType;
  /* 0x0050 */ struct _CM_KCB_UOW* ParentUoW;
  union {
    /* 0x0058 */ struct _CM_KEY_CONTROL_BLOCK* ChildKCB;
    /* 0x0058 */ unsigned long VolatileKeyCell;
    struct {
      /* 0x0058 */ unsigned long OldValueCell;
      /* 0x005c */ unsigned long NewValueCell;
    }; /* size: 0x0008 */
    /* 0x0058 */ unsigned long UserFlags;
    /* 0x0058 */ union _LARGE_INTEGER LastWriteTime;
    /* 0x0058 */ unsigned long TxSecurityCell;
    struct {
      /* 0x0058 */ struct _CM_KEY_CONTROL_BLOCK* OldChildKCB;
      /* 0x0060 */ struct _CM_KEY_CONTROL_BLOCK* NewChildKCB;
    }; /* size: 0x0010 */
    struct {
      /* 0x0058 */ struct _CM_KEY_CONTROL_BLOCK* OtherChildKCB;
      /* 0x0060 */ unsigned long ThisVolatileKeyCell;
    }; /* size: 0x000c */
  };   /* size: 0x0010 */
  union {
    /* 0x0068 */ void* PrepareDataPointer;
    /* 0x0068 */ struct _CM_UOW_SET_SD_DATA* SecurityData;
    /* 0x0068 */ struct _CM_UOW_KEY_STATE_MODIFICATION* ModifyKeysData;
    /* 0x0068 */ struct _CM_UOW_SET_VALUE_LIST_DATA* SetValueData;
  }; /* size: 0x0008 */
  union {
    /* 0x0070 */ struct _CM_UOW_SET_VALUE_KEY_DATA* ValueData;
    /* 0x0070 */ struct _CMP_DISCARD_AND_REPLACE_KCB_CONTEXT*
        DiscardReplaceContext;
  }; /* size: 0x0008 */
};

}  // namespace resym_test

int main() {
  using namespace resym_test;

  PrimitiveTypesTest primitive_types_test{};
  ArrayTest array_test{};
  BitFieldsTest1 bit_fields_test1{};
  BitFieldsTest2 bit_fields_test2{};
  BitFieldsTest3 bit_fields_test3{};
  BitFieldsTest4 bit_fields_test4{};
  BitFieldsTest5 bit_fields_test5{};
  BitFieldsTest6 bit_fields_test6{};
  BitFieldsTest7 bit_fields_test7{};
  UnionTest union_test{};
  StructTest struct_test{};
  EnumTest1 enum_test1{};
  EnumTest2 enum_test2{};
  EnumClassTest1 enum_class_test1{};
  EnumClassTest2 enum_class_test2{};
  InterfaceImplClass interface_impl_class{};
  SpecializedInterfaceImplClass specialized_interface_impl_class{};
  ClassWithRefsAndStaticsTest class_with_refs{};
  ClassWithNestedDeclarationsTest class_with_nested{};
  UnionWithNestedDeclarationsTest union_with_nested{};
  UnionUnnamedUdtTest1 union_with_unnamed_structs{};
  StructUnnamedUdtTest1 struct_with_unnamed_unions{};
  StructUnnamedUdtTest2 regression_test{};
  StructUnnamedUdtTest3 regression_test2{};
  StructAccessTest access_test1{};
  ClassAccessTest access_test2{};
  UnionAccessTest access_test3{};
  BigOffsetsStruct big_offsets{};
  NestedStructUnionRegression1 nested_regression1{};

  return 0;
}
