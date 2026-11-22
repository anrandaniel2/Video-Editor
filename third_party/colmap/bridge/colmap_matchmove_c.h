// SPDX-License-Identifier: BSD-3-Clause
#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ColmapInputFrame {
  double time_s;
  uint32_t width;
  uint32_t height;
  const uint8_t* gray;
} ColmapInputFrame;

typedef struct ColmapSolveConfig {
  double initial_focal_px;
  int lock_fov;
  int max_features;
  int use_opengl;
  int prefer_glomap;
} ColmapSolveConfig;

typedef struct ColmapSolvedPose {
  double time_s;
  int registered;
  double r_cw[9];
  double t_cw[3];
  double focal_px;
} ColmapSolvedPose;

typedef struct ColmapSolvedPoint3D {
  double xyz[3];
  uint32_t observations;
  double reproj_error;
} ColmapSolvedPoint3D;

typedef struct ColmapSolveResult {
  int status;
  int used_opengl;
  int used_glomap;
  double focal_px;
  double mean_reproj_px;
  size_t num_poses;
  ColmapSolvedPose* poses;
  size_t num_points;
  ColmapSolvedPoint3D* points;
  char error_msg[256];
} ColmapSolveResult;

int colmap_solve_sequence(const ColmapInputFrame* frames,
                          size_t num_frames,
                          const ColmapSolveConfig* config,
                          ColmapSolveResult* out_result);

void colmap_free_result(ColmapSolveResult* result);

#ifdef __cplusplus
}
#endif
