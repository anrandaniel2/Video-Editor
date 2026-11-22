// SPDX-License-Identifier: BSD-3-Clause
#pragma once

#include "colmap/feature/types.h"
#include "colmap/util/threading.h"

#include <cstdint>
#include <memory>
#include <vector>

namespace colmap {

// Headless OpenGL ES 3.1 / EGL context manager (no Qt, no windowing UI).
class OpenGLContextManager {
 public:
  explicit OpenGLContextManager(int opengl_major_version = 3,
                                int opengl_minor_version = 1);
  ~OpenGLContextManager();

  OpenGLContextManager(const OpenGLContextManager&) = delete;
  OpenGLContextManager& operator=(const OpenGLContextManager&) = delete;

  bool MakeCurrent();
  bool IsValid() const;

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

void RunThreadWithOpenGLContext(Thread* thread);

// Headless OpenGL ES 3.1 compute-shader SIFT extractor & descriptor matcher.
class OpenGLSiftEngine {
 public:
  OpenGLSiftEngine();
  ~OpenGLSiftEngine();

  bool Initialize();
  bool IsAvailable() const;

  bool Extract(const uint8_t* gray,
               int width,
               int height,
               int max_features,
               int num_octaves,
               int octave_resolution,
               float peak_threshold,
               float edge_threshold,
               bool l1_root_norm,
               FeatureKeypoints* keypoints,
               FeatureDescriptorsData* descriptors);

  bool Match(const FeatureDescriptorsData& desc1,
             const FeatureDescriptorsData& desc2,
             float max_ratio,
             float max_distance,
             bool cross_check,
             FeatureMatches* matches);

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

}  // namespace colmap
