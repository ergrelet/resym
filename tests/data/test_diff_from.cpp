struct UserStructAddAndReplace {
  int field1;
  char field2;
  void* field3;
};

struct UserStructRemove {
  int field1;
  char field2;
  void* field3;
  int field4;
};

struct UserStructAdd {
  int field1;
  void* field2;
};

int main() {
  UserStructAddAndReplace user_struct_add_and_replace{};
  UserStructRemove user_struct_remove{};
  UserStructAdd user_struct_add{};
  return 0;
}
