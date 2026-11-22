// SPDX-License-Identifier: BSD-3-Clause
#include "colmap/util/endian.h"

namespace colmap {

bool IsLittleEndian() {
#if defined(__BYTE_ORDER__) && defined(__ORDER_LITTLE_ENDIAN__)
  return __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__;
#else
  const uint16_t x = 1;
  return *reinterpret_cast<const uint8_t*>(&x) == 1;
#endif
}

bool IsBigEndian() { return !IsLittleEndian(); }

}  // namespace colmap
