// SPDX-License-Identifier: BSD-3-Clause
#include "colmap/feature/matcher.h"
#include "colmap/feature/sift.h"
#include "colmap/util/logging.h"
#include "colmap/util/misc.h"

namespace colmap {

FeatureMatchingTypeOptions::FeatureMatchingTypeOptions()
    : sift(std::make_shared<SiftMatchingOptions>()) {}

FeatureMatchingTypeOptions::FeatureMatchingTypeOptions(
    const FeatureMatchingTypeOptions& other)
    : sift(CloneSharedPtr(other.sift)) {}

FeatureMatchingTypeOptions& FeatureMatchingTypeOptions::operator=(
    const FeatureMatchingTypeOptions& other) {
  if (this != &other) sift = CloneSharedPtr(other.sift);
  return *this;
}

FeatureMatchingOptions::FeatureMatchingOptions(FeatureMatcherType type)
    : FeatureMatchingTypeOptions(), type(type) {}

bool FeatureMatchingOptions::UsesDescriptors() const { return true; }
bool FeatureMatchingOptions::RequiresOpenGL() const { return use_gpu; }

bool FeatureMatchingOptions::Check() const {
  CHECK_OPTION_GE(max_num_matches, 0);
  return THROW_CHECK_NOTNULL(sift)->Check();
}

std::unique_ptr<FeatureMatcher> FeatureMatcher::Create(
    const FeatureMatchingOptions& options) {
  return CreateSiftFeatureMatcher(options);
}

}  // namespace colmap
