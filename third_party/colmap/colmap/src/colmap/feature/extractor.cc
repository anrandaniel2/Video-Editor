// SPDX-License-Identifier: BSD-3-Clause
#include "colmap/feature/extractor.h"
#include "colmap/feature/sift.h"
#include "colmap/util/logging.h"
#include "colmap/util/misc.h"

namespace colmap {

FeatureExtractionTypeOptions::FeatureExtractionTypeOptions()
    : sift(std::make_shared<SiftExtractionOptions>()) {}

FeatureExtractionTypeOptions::FeatureExtractionTypeOptions(
    const FeatureExtractionTypeOptions& other)
    : sift(CloneSharedPtr(other.sift)) {}

FeatureExtractionTypeOptions& FeatureExtractionTypeOptions::operator=(
    const FeatureExtractionTypeOptions& other) {
  if (this != &other) sift = CloneSharedPtr(other.sift);
  return *this;
}

FeatureExtractionOptions::FeatureExtractionOptions(FeatureExtractorType type)
    : FeatureExtractionTypeOptions(), type(type) {}

bool FeatureExtractionOptions::RequiresRGB() const { return false; }

bool FeatureExtractionOptions::RequiresOpenGL() const {
  return use_gpu && !sift->estimate_affine_shape &&
         !sift->domain_size_pooling && !sift->force_covariant_extractor;
}

int FeatureExtractionOptions::EffMaxImageSize() const {
  return max_image_size > 0 ? max_image_size : 3200;
}

bool FeatureExtractionOptions::Check() const {
  CHECK_OPTION_GT(EffMaxImageSize(), 0);
  return THROW_CHECK_NOTNULL(sift)->Check();
}

std::unique_ptr<FeatureExtractor> FeatureExtractor::Create(
    const FeatureExtractionOptions& options) {
  return CreateSiftFeatureExtractor(options);
}

}  // namespace colmap
