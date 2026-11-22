// SPDX-License-Identifier: BSD-3-Clause
#pragma once

#include "colmap/feature/types.h"
#include "colmap/sensor/bitmap.h"

#include <memory>
#include <string>

namespace colmap {

struct SiftExtractionOptions;

struct FeatureExtractionTypeOptions {
  FeatureExtractionTypeOptions();
  std::shared_ptr<SiftExtractionOptions> sift;

  FeatureExtractionTypeOptions(const FeatureExtractionTypeOptions& other);
  FeatureExtractionTypeOptions& operator=(const FeatureExtractionTypeOptions& other);
  FeatureExtractionTypeOptions(FeatureExtractionTypeOptions&& other) = default;
  FeatureExtractionTypeOptions& operator=(FeatureExtractionTypeOptions&& other) = default;
};

struct FeatureExtractionOptions : public FeatureExtractionTypeOptions {
  explicit FeatureExtractionOptions(
      FeatureExtractorType type = FeatureExtractorType::SIFT);

  FeatureExtractorType type = FeatureExtractorType::SIFT;
  int max_image_size = -1;
  int num_threads = -1;
  bool use_gpu = true;
  std::string gpu_index = "-1";

  bool RequiresRGB() const;
  bool RequiresOpenGL() const;
  int EffMaxImageSize() const;
  bool Check() const;
};

class FeatureExtractor {
 public:
  virtual ~FeatureExtractor() = default;
  static std::unique_ptr<FeatureExtractor> Create(
      const FeatureExtractionOptions& options);
  virtual bool Extract(const Bitmap& bitmap,
                       FeatureKeypoints* keypoints,
                       FeatureDescriptors* descriptors) = 0;
};

}  // namespace colmap
