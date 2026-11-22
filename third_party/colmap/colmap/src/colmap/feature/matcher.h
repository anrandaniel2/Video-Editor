// SPDX-License-Identifier: BSD-3-Clause
#pragma once

#include "colmap/feature/types.h"
#include "colmap/geometry/pose_prior.h"
#include "colmap/scene/camera.h"
#include "colmap/scene/two_view_geometry.h"
#include "colmap/util/types.h"

#include <memory>
#include <string>
#include <string_view>

namespace colmap {

struct SiftMatchingOptions;

struct FeatureMatchingTypeOptions {
  FeatureMatchingTypeOptions();
  std::shared_ptr<SiftMatchingOptions> sift;

  FeatureMatchingTypeOptions(const FeatureMatchingTypeOptions& other);
  FeatureMatchingTypeOptions& operator=(const FeatureMatchingTypeOptions& other);
  FeatureMatchingTypeOptions(FeatureMatchingTypeOptions&& other) = default;
  FeatureMatchingTypeOptions& operator=(FeatureMatchingTypeOptions&& other) = default;
};

struct FeatureMatchingOptions : public FeatureMatchingTypeOptions {
  explicit FeatureMatchingOptions(
      FeatureMatcherType type = FeatureMatcherType::SIFT_BRUTEFORCE);

  FeatureMatcherType type = FeatureMatcherType::SIFT_BRUTEFORCE;
  int num_threads = -1;
  bool use_gpu = true;
  std::string gpu_index = "-1";
  int max_num_matches = 32768;
  bool guided_matching = false;
  bool skip_geometric_verification = false;
  bool rig_verification = false;
  bool skip_image_pairs_in_same_frame = false;

  bool RequiresOpenGL() const;
  bool UsesDescriptors() const;
  bool Check() const;
};

class FeatureMatcher {
 public:
  virtual ~FeatureMatcher() = default;

  struct Image {
    image_t image_id = kInvalidImageId;
    const Camera* camera = nullptr;
    std::shared_ptr<const FeatureKeypoints> keypoints;
    std::shared_ptr<const FeatureDescriptors> descriptors;
    const PosePrior* pose_prior = nullptr;
    std::string_view name;
  };

  static std::unique_ptr<FeatureMatcher> Create(
      const FeatureMatchingOptions& options);

  virtual void Match(const Image& image1,
                     const Image& image2,
                     FeatureMatches* matches) = 0;

  virtual void MatchGuided(double max_error,
                           const Image& image1,
                           const Image& image2,
                           TwoViewGeometry* two_view_geometry) = 0;
};

}  // namespace colmap
