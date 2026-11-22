// SPDX-License-Identifier: BSD-3-Clause
#pragma once

#include "colmap/feature/extractor.h"
#include "colmap/feature/index.h"
#include "colmap/feature/matcher.h"
#include "colmap/util/cache.h"
#include "colmap/util/enum_utils.h"

#include <filesystem>

namespace colmap {

struct SiftExtractionOptions {
  int max_num_features = 8192;
  int first_octave = -1;
  int num_octaves = 4;
  int octave_resolution = 3;
  double peak_threshold = 0.02 / octave_resolution;
  double edge_threshold = 10.0;
  bool estimate_affine_shape = false;
  int max_num_orientations = 2;
  bool upright = false;
  bool darkness_adaptivity = false;
  bool domain_size_pooling = false;
  double dsp_min_scale = 1.0 / 6.0;
  double dsp_max_scale = 3.0;
  int dsp_num_scales = 10;
  bool force_covariant_extractor = false;

  MAKE_ENUM_CLASS(Normalization, 0, L1_ROOT, L2);
  Normalization normalization = Normalization::L1_ROOT;

  bool Check() const;
};

std::unique_ptr<FeatureExtractor> CreateSiftFeatureExtractor(
    const FeatureExtractionOptions& options);

struct SiftMatchingOptions {
  double max_ratio = 0.8;
  double max_distance = 0.7;
  bool cross_check = true;
  bool cpu_brute_force_matcher = false;
  ThreadSafeLRUCache<image_t, FeatureDescriptorIndex>*
      cpu_descriptor_index_cache = nullptr;

  bool Check() const;
};

std::unique_ptr<FeatureMatcher> CreateSiftFeatureMatcher(
    const FeatureMatchingOptions& options);

void LoadSiftFeaturesFromTextFile(const std::filesystem::path& path,
                                  FeatureKeypoints* keypoints,
                                  FeatureDescriptors* descriptors);

}  // namespace colmap
