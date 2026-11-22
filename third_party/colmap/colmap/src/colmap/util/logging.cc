// SPDX-License-Identifier: BSD-3-Clause
#include "colmap/util/logging.h"

namespace colmap {

void InitializeGlog(char** argv) {
  if (argv) {
    google::InitGoogleLogging(argv[0]);
  }
}

const char* __GetConstFileBaseName(const char* file) {
  const char* base = std::strrchr(file, '/');
  if (!base) {
    base = std::strrchr(file, '\\');
  }
  return base ? (base + 1) : file;
}

bool __CheckOptionImpl(const char* file,
                       const int line,
                       const bool result,
                       const char* expr_str) {
  if (result) {
    return true;
  }
  LOG(ERROR) << StringPrintf("[%s:%d] Check failed: %s",
                             __GetConstFileBaseName(file),
                             line,
                             expr_str);
  return false;
}

}  // namespace colmap
