struct NewStruct {
  int field;
};

struct UserStructAddAndReplace {
  int before1;
  int field1;
  int between12;
  char field2;
  int between23;
  void* field3;
  int after3;
};

struct UserStructRemove {
  int field1;
  void* field3;
};

struct UserStructAdd {
  int field1;
  void* field2;
  char field3;
  int field4;
};

int main() {
  NewStruct new_struct{};
  UserStructAddAndReplace user_struct_add_and_replace{};
  UserStructRemove user_struct_remove{};
  UserStructAdd user_struct_add{};
  return 0;
}