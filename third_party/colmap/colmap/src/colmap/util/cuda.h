// SPDX-License-Identifier: BSD-3-Clause
#pragma once

namespace colmap {

inline int GetNumCudaDevices() { return 0; }
inline int FindBestCudaDevice() { return -1; }
inline void SetBestCudaDevice(int) {}
inline bool CudaDeviceHasComputeCapability(int, int) { return false; }

}  // namespace colmap
