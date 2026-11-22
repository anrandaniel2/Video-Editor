// SPDX-License-Identifier: BSD-3-Clause

#include "colmap/feature/sift.h"

#include "colmap/feature/utils.h"
#include "colmap/geometry/essential_matrix.h"
#include "colmap/math/math.h"
#include "colmap/util/file.h"
#include "colmap/util/logging.h"
#include "colmap/util/misc.h"
#include "colmap/util/opengl_utils.h"
#include "colmap/util/string.h"
#include "colmap/util/eigen_alignment.h"

extern "C" {
#include "thirdparty/VLFeat/covdet.h"
#include "thirdparty/VLFeat/sift.h"
}

#include <array>
#include <fstream>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <sstream>

#include <Eigen/Geometry>

namespace colmap {

constexpr int kSiftDescriptorDim = 128;
constexpr int kSqSiftDescriptorNorm = 512 * 512;

bool SiftExtractionOptions::Check() const {
  CHECK_OPTION_GT(max_num_features, 0);
  CHECK_OPTION_GT(octave_resolution, 0);
  CHECK_OPTION_GT(peak_threshold, 0.0);
  CHECK_OPTION_GT(edge_threshold, 0.0);
  CHECK_OPTION_GT(max_num_orientations, 0);
  if (domain_size_pooling) {
    CHECK_OPTION_GT(dsp_min_scale, 0);
    CHECK_OPTION_GE(dsp_max_scale, dsp_min_scale);
    CHECK_OPTION_GT(dsp_num_scales, 0);
  }
  return true;
}

bool SiftMatchingOptions::Check() const {
  CHECK_OPTION_GT(max_ratio, 0.0);
  CHECK_OPTION_GT(max_distance, 0.0);
  return true;
}

namespace {

void WarnDarknessAdaptivityNotAvailable() {
  LOG(WARNING) << "Darkness adaptivity only available for GLSL SiftGPU.";
}

void ThrowCheckFeatureTypesMatch(const FeatureMatcher::Image& image1,
                                 const FeatureMatcher::Image& image2,
                                 bool check_keypoints = false) {
  THROW_CHECK_NOTNULL(image1.descriptors);
  THROW_CHECK_NOTNULL(image2.descriptors);
  THROW_CHECK_EQ(image1.descriptors->type, FeatureExtractorType::SIFT);
  THROW_CHECK_EQ(image2.descriptors->type, FeatureExtractorType::SIFT);
  THROW_CHECK_EQ(image1.descriptors->data.cols(), kSiftDescriptorDim);
  THROW_CHECK_EQ(image2.descriptors->data.cols(), kSiftDescriptorDim);
  if (check_keypoints) {
    THROW_CHECK_NOTNULL(image1.camera);
    THROW_CHECK_NOTNULL(image2.camera);
    THROW_CHECK_NOTNULL(image1.keypoints);
    THROW_CHECK_NOTNULL(image2.keypoints);
    THROW_CHECK_EQ(image1.descriptors->data.rows(), image1.keypoints->size());
    THROW_CHECK_EQ(image2.descriptors->data.rows(), image2.keypoints->size());
  }
}

// VLFeat uses a different convention to store its descriptors. This transforms
// the VLFeat format into the original SIFT format that is also used by SiftGPU.
FeatureDescriptorsData TransformVLFeatToUBCFeatureDescriptors(
    const FeatureDescriptorsData& vlfeat_descriptors) {
  FeatureDescriptorsData ubc_descriptors(vlfeat_descriptors.rows(),
                                         vlfeat_descriptors.cols());
  const std::array<int, 8> q{{0, 7, 6, 5, 4, 3, 2, 1}};
  for (Eigen::Index n = 0; n < vlfeat_descriptors.rows(); ++n) {
    for (int i = 0; i < 4; ++i) {
      for (int j = 0; j < 4; ++j) {
        for (int k = 0; k < 8; ++k) {
          ubc_descriptors(n, 8 * (j + 4 * i) + q[k]) =
              vlfeat_descriptors(n, 8 * (j + 4 * i) + k);
        }
      }
    }
  }
  return ubc_descriptors;
}

class SiftCPUFeatureExtractor : public FeatureExtractor {
 public:
  using VlSiftType = std::unique_ptr<VlSiftFilt, void (*)(VlSiftFilt*)>;

  explicit SiftCPUFeatureExtractor(const FeatureExtractionOptions& options)
      : options_(options), sift_(nullptr, &vl_sift_delete) {
    THROW_CHECK(options_.Check());
    THROW_CHECK(!options_.sift->estimate_affine_shape);
    THROW_CHECK(!options_.sift->domain_size_pooling);
    THROW_CHECK(!options_.sift->force_covariant_extractor);
    if (options_.sift->darkness_adaptivity) {
      WarnDarknessAdaptivityNotAvailable();
    }
  }

  static std::unique_ptr<FeatureExtractor> Create(
      const FeatureExtractionOptions& options) {
    return std::make_unique<SiftCPUFeatureExtractor>(options);
  }

  bool Extract(const Bitmap& bitmap,
               FeatureKeypoints* keypoints,
               FeatureDescriptors* descriptors) {
    THROW_CHECK(bitmap.IsGrey());
    THROW_CHECK_NOTNULL(keypoints);

    if (sift_ == nullptr || sift_->width != bitmap.Width() ||
        sift_->height != bitmap.Height()) {
      sift_ = VlSiftType(vl_sift_new(bitmap.Width(),
                                     bitmap.Height(),
                                     options_.sift->num_octaves,
                                     options_.sift->octave_resolution,
                                     options_.sift->first_octave),
                         &vl_sift_delete);
      if (!sift_) {
        return false;
      }
    }

    vl_sift_set_peak_thresh(sift_.get(), options_.sift->peak_threshold);
    vl_sift_set_edge_thresh(sift_.get(), options_.sift->edge_threshold);

    // Iterate through octaves.
    std::vector<size_t> level_num_features;
    std::vector<FeatureKeypoints> level_keypoints;
    std::vector<FeatureDescriptorsData> level_descriptors;
    bool first_octave = true;
    while (true) {
      if (first_octave) {
        const std::vector<uint8_t>& data_uint8 = bitmap.RowMajorData();
        std::vector<float> data_float(data_uint8.size());
        for (size_t i = 0; i < data_uint8.size(); ++i) {
          data_float[i] = static_cast<float>(data_uint8[i]) / 255.0f;
        }
        if (vl_sift_process_first_octave(sift_.get(), data_float.data())) {
          break;
        }
        first_octave = false;
      } else {
        if (vl_sift_process_next_octave(sift_.get())) {
          break;
        }
      }

      // Detect keypoints.
      vl_sift_detect(sift_.get());

      // Extract detected keypoints.
      const VlSiftKeypoint* vl_keypoints = vl_sift_get_keypoints(sift_.get());
      const int num_keypoints = vl_sift_get_nkeypoints(sift_.get());
      if (num_keypoints == 0) {
        continue;
      }

      // Extract features with different orientations per DOG level.
      size_t level_idx = 0;
      int prev_level = -1;
      FeatureDescriptorsFloatData desc(1, kSiftDescriptorDim);
      for (int i = 0; i < num_keypoints; ++i) {
        if (vl_keypoints[i].is != prev_level) {
          if (i > 0) {
            // Resize containers of previous DOG level.
            level_keypoints.back().resize(level_idx);
            if (descriptors != nullptr) {
              level_descriptors.back().conservativeResize(level_idx,
                                                          kSiftDescriptorDim);
            }
          }

          // Add containers for new DOG level.
          level_idx = 0;
          level_num_features.push_back(0);
          level_keypoints.emplace_back(options_.sift->max_num_orientations *
                                       num_keypoints);
          if (descriptors != nullptr) {
            level_descriptors.emplace_back(
                options_.sift->max_num_orientations * num_keypoints,
                kSiftDescriptorDim);
          }
        }

        level_num_features.back() += 1;
        prev_level = vl_keypoints[i].is;

        // Extract feature orientations.
        double angles[4];
        int num_orientations;
        if (options_.sift->upright) {
          num_orientations = 1;
          angles[0] = 0.0;
        } else {
          num_orientations = vl_sift_calc_keypoint_orientations(
              sift_.get(), angles, &vl_keypoints[i]);
        }

        // Note that this is different from SiftGPU, which selects the top
        // global maxima as orientations while this selects the first two
        // local maxima. It is not clear which procedure is better.
        const int num_used_orientations =
            std::min(num_orientations, options_.sift->max_num_orientations);

        for (int o = 0; o < num_used_orientations; ++o) {
          level_keypoints.back()[level_idx] =
              FeatureKeypoint(vl_keypoints[i].x + 0.5f,
                              vl_keypoints[i].y + 0.5f,
                              vl_keypoints[i].sigma,
                              angles[o]);
          if (descriptors != nullptr) {
            vl_sift_calc_keypoint_descriptor(
                sift_.get(), desc.data(), &vl_keypoints[i], angles[o]);
            if (options_.sift->normalization ==
                SiftExtractionOptions::Normalization::L2) {
              L2NormalizeFeatureDescriptors(&desc);
            } else if (options_.sift->normalization ==
                       SiftExtractionOptions::Normalization::L1_ROOT) {
              L1RootNormalizeFeatureDescriptors(&desc);
            } else {
              LOG(FATAL_THROW) << "Normalization type not supported";
            }

            level_descriptors.back().row(level_idx) =
                FeatureDescriptorsToUnsignedByte(desc);
          }

          level_idx += 1;
        }
      }

      // Resize containers for last DOG level in octave.
      level_keypoints.back().resize(level_idx);
      if (descriptors != nullptr) {
        level_descriptors.back().conservativeResize(level_idx,
                                                    kSiftDescriptorDim);
      }
    }

    // Determine how many DOG levels to keep to satisfy max_num_features option.
    int first_level_to_keep = 0;
    int num_features = 0;
    int num_features_with_orientations = 0;
    for (int i = level_keypoints.size() - 1; i >= 0; --i) {
      num_features += level_num_features[i];
      num_features_with_orientations += level_keypoints[i].size();
      if (num_features > options_.sift->max_num_features) {
        first_level_to_keep = i;
        break;
      }
    }

    // Extract the features to be kept.
    {
      size_t k = 0;
      keypoints->resize(num_features_with_orientations);
      for (size_t i = first_level_to_keep; i < level_keypoints.size(); ++i) {
        for (size_t j = 0; j < level_keypoints[i].size(); ++j) {
          (*keypoints)[k] = level_keypoints[i][j];
          k += 1;
        }
      }
    }

    // Compute the descriptors for the detected keypoints.
    if (descriptors != nullptr) {
      size_t k = 0;
      descriptors->data.resize(num_features_with_orientations,
                               kSiftDescriptorDim);
      for (size_t i = first_level_to_keep; i < level_keypoints.size(); ++i) {
        for (size_t j = 0; j < level_keypoints[i].size(); ++j) {
          descriptors->data.row(k) = level_descriptors[i].row(j);
          k += 1;
        }
      }
      descriptors->data =
          TransformVLFeatToUBCFeatureDescriptors(descriptors->data);
      descriptors->type = FeatureExtractorType::SIFT;
    }

    return true;
  }

 private:
  const FeatureExtractionOptions options_;
  VlSiftType sift_;
};

class CovariantSiftCPUFeatureExtractor : public FeatureExtractor {
 public:
  explicit CovariantSiftCPUFeatureExtractor(
      const FeatureExtractionOptions& options)
      : options_(options) {
    THROW_CHECK(options_.Check());
    if (options_.sift->darkness_adaptivity) {
      WarnDarknessAdaptivityNotAvailable();
    }
  }

  static std::unique_ptr<FeatureExtractor> Create(
      const FeatureExtractionOptions& options) {
    return std::make_unique<CovariantSiftCPUFeatureExtractor>(options);
  }

  bool Extract(const Bitmap& bitmap,
               FeatureKeypoints* keypoints,
               FeatureDescriptors* descriptors) {
    THROW_CHECK(bitmap.IsGrey());
    THROW_CHECK_NOTNULL(keypoints);

    // Setup covariant SIFT detector.
    std::unique_ptr<VlCovDet, void (*)(VlCovDet*)> covdet(
        vl_covdet_new(VL_COVDET_METHOD_DOG), &vl_covdet_delete);
    if (!covdet) {
      return false;
    }

    const int kMaxOctaveResolution = 1000;
    THROW_CHECK_LE(options_.sift->octave_resolution, kMaxOctaveResolution);

    vl_covdet_set_first_octave(covdet.get(), options_.sift->first_octave);
    vl_covdet_set_octave_resolution(covdet.get(),
                                    options_.sift->octave_resolution);
    vl_covdet_set_peak_threshold(covdet.get(), options_.sift->peak_threshold);
    vl_covdet_set_edge_threshold(covdet.get(), options_.sift->edge_threshold);

    {
      const std::vector<uint8_t>& data_uint8 = bitmap.RowMajorData();
      std::vector<float> data_float(data_uint8.size());
      for (size_t i = 0; i < data_uint8.size(); ++i) {
        data_float[i] = static_cast<float>(data_uint8[i]) / 255.0f;
      }
      vl_covdet_put_image(
          covdet.get(), data_float.data(), bitmap.Width(), bitmap.Height());
    }

    vl_covdet_detect(covdet.get(), options_.sift->max_num_features);

    if (options_.sift->estimate_affine_shape) {
      vl_covdet_extract_affine_shape(covdet.get());
    }

    if (!options_.sift->upright) {
      vl_covdet_extract_orientations(covdet.get());
    }

    const int num_features = vl_covdet_get_num_features(covdet.get());
    VlCovDetFeature* features = vl_covdet_get_features(covdet.get());

    // Sort features according to detected octave and scale.
    std::sort(
        features,
        features + num_features,
        [](const VlCovDetFeature& feature1, const VlCovDetFeature& feature2) {
          if (feature1.o == feature2.o) {
            return feature1.s > feature2.s;
          } else {
            return feature1.o > feature2.o;
          }
        });

    const size_t max_num_features =
        static_cast<size_t>(options_.sift->max_num_features);

    // Copy detected keypoints and clamp when maximum number of features
    // reached.
    int prev_octave_scale_idx = std::numeric_limits<int>::max();
    for (int i = 0; i < num_features; ++i) {
      FeatureKeypoint keypoint;
      keypoint.x = features[i].frame.x + 0.5;
      keypoint.y = features[i].frame.y + 0.5;
      keypoint.a11 = features[i].frame.a11;
      keypoint.a12 = features[i].frame.a12;
      keypoint.a21 = features[i].frame.a21;
      keypoint.a22 = features[i].frame.a22;
      keypoints->push_back(keypoint);

      const int octave_scale_idx =
          features[i].o * kMaxOctaveResolution + features[i].s;
      THROW_CHECK_LE(octave_scale_idx, prev_octave_scale_idx);

      if (octave_scale_idx != prev_octave_scale_idx &&
          keypoints->size() >= max_num_features) {
        break;
      }

      prev_octave_scale_idx = octave_scale_idx;
    }

    // Compute the descriptors for the detected keypoints.
    if (descriptors != nullptr) {
      descriptors->data.resize(keypoints->size(), kSiftDescriptorDim);

      const size_t kPatchResolution = 15;
      const size_t kPatchSide = 2 * kPatchResolution + 1;
      const double kPatchRelativeExtent = 7.5;
      const double kPatchRelativeSmoothing = 1;
      const double kPatchStep = kPatchRelativeExtent / kPatchResolution;
      const double kSigma =
          kPatchRelativeExtent / (3.0 * (4 + 1) / 2) / kPatchStep;

      std::vector<float> patch(kPatchSide * kPatchSide);
      std::vector<float> patchXY(2 * kPatchSide * kPatchSide);

      float dsp_min_scale = 1;
      float dsp_scale_step = 0;
      int dsp_num_scales = 1;
      if (options_.sift->domain_size_pooling) {
        dsp_min_scale = options_.sift->dsp_min_scale;
        dsp_scale_step =
            (options_.sift->dsp_max_scale - options_.sift->dsp_min_scale) /
            options_.sift->dsp_num_scales;
        dsp_num_scales = options_.sift->dsp_num_scales;
      }

      FeatureDescriptorsFloatData descriptor(1, kSiftDescriptorDim);
      FeatureDescriptorsFloatData scaled_descriptors(dsp_num_scales,
                                                     kSiftDescriptorDim);

      std::unique_ptr<VlSiftFilt, void (*)(VlSiftFilt*)> sift(
          vl_sift_new(16, 16, 1, 3, 0), &vl_sift_delete);
      if (!sift) {
        return false;
      }

      vl_sift_set_magnif(sift.get(), 3.0);

      for (size_t i = 0; i < keypoints->size(); ++i) {
        for (int s = 0; s < dsp_num_scales; ++s) {
          const double dsp_scale = dsp_min_scale + s * dsp_scale_step;

          VlFrameOrientedEllipse scaled_frame = features[i].frame;
          scaled_frame.a11 *= dsp_scale;
          scaled_frame.a12 *= dsp_scale;
          scaled_frame.a21 *= dsp_scale;
          scaled_frame.a22 *= dsp_scale;

          vl_covdet_extract_patch_for_frame(covdet.get(),
                                            patch.data(),
                                            kPatchResolution,
                                            kPatchRelativeExtent,
                                            kPatchRelativeSmoothing,
                                            scaled_frame);

          vl_imgradient_polar_f(patchXY.data(),
                                patchXY.data() + 1,
                                2,
                                2 * kPatchSide,
                                patch.data(),
                                kPatchSide,
                                kPatchSide,
                                kPatchSide);

          vl_sift_calc_raw_descriptor(sift.get(),
                                      patchXY.data(),
                                      scaled_descriptors.row(s).data(),
                                      kPatchSide,
                                      kPatchSide,
                                      kPatchResolution,
                                      kPatchResolution,
                                      kSigma,
                                      0);
        }

        if (options_.sift->domain_size_pooling) {
          descriptor = scaled_descriptors.colwise().mean();
        } else {
          descriptor = scaled_descriptors;
        }

        THROW_CHECK_EQ(descriptor.cols(), kSiftDescriptorDim);

        if (options_.sift->normalization ==
            SiftExtractionOptions::Normalization::L2) {
          L2NormalizeFeatureDescriptors(&descriptor);
        } else if (options_.sift->normalization ==
                   SiftExtractionOptions::Normalization::L1_ROOT) {
          L1RootNormalizeFeatureDescriptors(&descriptor);
        } else {
          LOG(FATAL_THROW) << "Normalization type not supported";
        }

        descriptors->data.row(i) = FeatureDescriptorsToUnsignedByte(descriptor);
      }

      descriptors->data =
          TransformVLFeatToUBCFeatureDescriptors(descriptors->data);
      descriptors->type = FeatureExtractorType::SIFT;
    }

    return true;
  }

 private:
  const FeatureExtractionOptions options_;
};


class SiftOpenGLFeatureExtractor : public FeatureExtractor {
 public:
  explicit SiftOpenGLFeatureExtractor(const FeatureExtractionOptions& options)
      : options_(options), cpu_fallback_(options) {}

  static std::unique_ptr<FeatureExtractor> Create(
      const FeatureExtractionOptions& options) {
    auto ext = std::make_unique<SiftOpenGLFeatureExtractor>(options);
    ext->gl_engine_.Initialize();
    return ext;
  }

  bool Extract(const Bitmap& bitmap,
               FeatureKeypoints* keypoints,
               FeatureDescriptors* descriptors) override {
    THROW_CHECK(bitmap.IsGrey());
    THROW_CHECK_NOTNULL(keypoints);
    THROW_CHECK_NOTNULL(descriptors);
    if (gl_engine_.IsAvailable()) {
      const bool l1_root = options_.sift->normalization ==
                           SiftExtractionOptions::Normalization::L1_ROOT;
      if (gl_engine_.Extract(bitmap.RowMajorData().data(),
                             bitmap.Width(),
                             bitmap.Height(),
                             options_.sift->max_num_features,
                             options_.sift->num_octaves,
                             options_.sift->octave_resolution,
                             static_cast<float>(options_.sift->peak_threshold),
                             static_cast<float>(options_.sift->edge_threshold),
                             l1_root,
                             keypoints,
                             &descriptors->data)) {
        descriptors->type = FeatureExtractorType::SIFT;
        return true;
      }
    }
    return cpu_fallback_.Extract(bitmap, keypoints, descriptors);
  }

 private:
  const FeatureExtractionOptions options_;
  OpenGLSiftEngine gl_engine_;
  SiftCPUFeatureExtractor cpu_fallback_;
};

}  // namespace

std::unique_ptr<FeatureExtractor> CreateSiftFeatureExtractor(
    const FeatureExtractionOptions& options) {
  if (options.sift->estimate_affine_shape ||
      options.sift->domain_size_pooling ||
      options.sift->force_covariant_extractor) {
    return CovariantSiftCPUFeatureExtractor::Create(options);
  }
  if (options.use_gpu) {
    return SiftOpenGLFeatureExtractor::Create(options);
  }
  return SiftCPUFeatureExtractor::Create(options);
}

namespace {

size_t FindBestMatchesOneWayBruteForce(
    const Eigen::RowMajorMatrixXf& dot_products,
    const float max_ratio,
    const float max_distance,
    std::vector<int>* matches) {
  constexpr float kInvSqDescriptorNorm =
      static_cast<float>(1. / kSqSiftDescriptorNorm);

  size_t num_matches = 0;
  matches->resize(dot_products.rows(), -1);

  for (Eigen::Index i1 = 0; i1 < dot_products.rows(); ++i1) {
    int best_d2_idx = -1;
    float best_dot_product = 0;
    float second_best_dot_product = 0;
    for (Eigen::Index i2 = 0; i2 < dot_products.cols(); ++i2) {
      const float dot_product = dot_products(i1, i2);
      if (dot_product > best_dot_product) {
        best_d2_idx = i2;
        second_best_dot_product = best_dot_product;
        best_dot_product = dot_product;
      } else if (dot_product > second_best_dot_product) {
        second_best_dot_product = dot_product;
      }
    }

    // Check if any match found.
    if (best_d2_idx == -1) {
      continue;
    }

    // Convert to L2 distance in which the thresholds are defined.
    const float best_dist_normed =
        std::acos(std::min(kInvSqDescriptorNorm * best_dot_product, 1.0f));

    // Check if match distance passes threshold.
    if (best_dist_normed > max_distance) {
      continue;
    }

    const float second_best_dist_normed = std::acos(
        std::min(kInvSqDescriptorNorm * second_best_dot_product, 1.0f));

    // Check if match passes ratio test. Keep this comparison >= in order to
    // ensure that the case of best == second_best is detected.
    if (best_dist_normed >= max_ratio * second_best_dist_normed) {
      continue;
    }

    ++num_matches;
    (*matches)[i1] = best_d2_idx;
  }

  return num_matches;
}

void FindBestMatchesBruteForce(const Eigen::RowMajorMatrixXf& dot_products,
                               const float max_ratio,
                               const float max_distance,
                               const bool cross_check,
                               FeatureMatches* matches) {
  matches->clear();

  std::vector<int> matches_1to2;
  const size_t num_matches_1to2 = FindBestMatchesOneWayBruteForce(
      dot_products, max_ratio, max_distance, &matches_1to2);

  if (cross_check) {
    std::vector<int> matches_2to1;
    const size_t num_matches_2to1 = FindBestMatchesOneWayBruteForce(
        dot_products.transpose(), max_ratio, max_distance, &matches_2to1);
    matches->reserve(std::min(num_matches_1to2, num_matches_2to1));
    for (size_t i1 = 0; i1 < matches_1to2.size(); ++i1) {
      if (matches_1to2[i1] != -1 && matches_2to1[matches_1to2[i1]] != -1 &&
          matches_2to1[matches_1to2[i1]] == static_cast<int>(i1)) {
        FeatureMatch match;
        match.point2D_idx1 = i1;
        match.point2D_idx2 = matches_1to2[i1];
        matches->push_back(match);
      }
    }
  } else {
    matches->reserve(num_matches_1to2);
    for (size_t i1 = 0; i1 < matches_1to2.size(); ++i1) {
      if (matches_1to2[i1] != -1) {
        FeatureMatch match;
        match.point2D_idx1 = i1;
        match.point2D_idx2 = matches_1to2[i1];
        matches->push_back(match);
      }
    }
  }
}

size_t FindBestMatchesOneWayIndex(const Eigen::RowMajorMatrixXi& indices,
                                  const Eigen::RowMajorMatrixXf& l2_dists,
                                  const float max_ratio,
                                  const float max_distance,
                                  std::vector<int>* matches) {
  const float max_l2_dist = kSqSiftDescriptorNorm * max_distance * max_distance;

  size_t num_matches = 0;
  matches->resize(indices.rows(), -1);

  for (int d1_idx = 0; d1_idx < indices.rows(); ++d1_idx) {
    int best_d2_idx = -1;
    float best_l2_dist = std::numeric_limits<float>::max();
    float second_best_l2_dist = std::numeric_limits<float>::max();
    for (int n_idx = 0; n_idx < indices.cols(); ++n_idx) {
      const int d2_idx = indices(d1_idx, n_idx);
      const float l2_dist = l2_dists(d1_idx, n_idx);
      if (l2_dist < best_l2_dist) {
        best_d2_idx = d2_idx;
        second_best_l2_dist = best_l2_dist;
        best_l2_dist = l2_dist;
      } else if (l2_dist < second_best_l2_dist) {
        second_best_l2_dist = l2_dist;
      }
    }

    // Check if any match found.
    if (best_d2_idx == -1) {
      continue;
    }

    // Check if match distance passes threshold.
    if (best_l2_dist > max_l2_dist) {
      continue;
    }

    // Check if match passes ratio test. Keep this comparison >= in order to
    // ensure that the case of best == second_best is detected.
    if (std::sqrt(best_l2_dist) >= max_ratio * std::sqrt(second_best_l2_dist)) {
      continue;
    }

    ++num_matches;
    (*matches)[d1_idx] = best_d2_idx;
  }

  return num_matches;
}

void FindBestMatchesIndex(const Eigen::RowMajorMatrixXi& indices_1to2,
                          const Eigen::RowMajorMatrixXf& l2_dists_1to2,
                          const Eigen::RowMajorMatrixXi& indices_2to1,
                          const Eigen::RowMajorMatrixXf& l2_dists_2to1,
                          const float max_ratio,
                          const float max_distance,
                          const bool cross_check,
                          FeatureMatches* matches) {
  matches->clear();

  std::vector<int> matches_1to2;
  const size_t num_matches_1to2 = FindBestMatchesOneWayIndex(
      indices_1to2, l2_dists_1to2, max_ratio, max_distance, &matches_1to2);

  if (cross_check && indices_2to1.rows()) {
    std::vector<int> matches_2to1;
    const size_t num_matches_2to1 = FindBestMatchesOneWayIndex(
        indices_2to1, l2_dists_2to1, max_ratio, max_distance, &matches_2to1);
    matches->reserve(std::min(num_matches_1to2, num_matches_2to1));
    for (size_t i1 = 0; i1 < matches_1to2.size(); ++i1) {
      if (matches_1to2[i1] != -1 && matches_2to1[matches_1to2[i1]] != -1 &&
          matches_2to1[matches_1to2[i1]] == static_cast<int>(i1)) {
        FeatureMatch match;
        match.point2D_idx1 = i1;
        match.point2D_idx2 = matches_1to2[i1];
        matches->push_back(match);
      }
    }
  } else {
    matches->reserve(num_matches_1to2);
    for (size_t i1 = 0; i1 < matches_1to2.size(); ++i1) {
      if (matches_1to2[i1] != -1) {
        FeatureMatch match;
        match.point2D_idx1 = i1;
        match.point2D_idx2 = matches_1to2[i1];
        matches->push_back(match);
      }
    }
  }
}

enum class DistanceType {
  L2,
  DOT_PRODUCT,
};

// Computes the pairwise descriptor distance matrix. When `guided_filter` is
// given, it is called with the pair of descriptor indices and returns whether
// the pair should be rejected on geometric grounds; rejected pairs get the
// worst possible distance. Passing indices rather than keypoint coordinates
// lets the caller filter on whatever geometry it needs - pixels, bearings, or
// bearings plus their Jacobians - without this function knowing any of it.
Eigen::RowMajorMatrixXf ComputeSiftDistanceMatrix(
    const DistanceType distance_type,
    const FeatureDescriptorsData& descriptors1,
    const FeatureDescriptorsData& descriptors2,
    const std::function<bool(Eigen::Index, Eigen::Index)>& guided_filter) {
  const Eigen::Matrix<int, Eigen::Dynamic, kSiftDescriptorDim>
      descriptors1_int = descriptors1.cast<int>();
  const Eigen::Matrix<int, Eigen::Dynamic, kSiftDescriptorDim>
      descriptors2_int = descriptors2.cast<int>();

  Eigen::RowMajorMatrixXf distances(descriptors1.rows(), descriptors2.rows());
  for (Eigen::Index i1 = 0; i1 < descriptors1.rows(); ++i1) {
    for (Eigen::Index i2 = 0; i2 < descriptors2.rows(); ++i2) {
      if (guided_filter != nullptr && guided_filter(i1, i2)) {
        if (distance_type == DistanceType::L2) {
          distances(i1, i2) = kSqSiftDescriptorNorm;
        } else if (distance_type == DistanceType::DOT_PRODUCT) {
          distances(i1, i2) = 0;
        } else {
          LOG(FATAL_THROW) << "Distance type not supported";
        }
      } else {
        if (distance_type == DistanceType::L2) {
          distances(i1, i2) =
              (descriptors1_int.row(i1) - descriptors2_int.row(i2))
                  .squaredNorm();
        } else if (distance_type == DistanceType::DOT_PRODUCT) {
          distances(i1, i2) =
              descriptors1_int.row(i1).dot(descriptors2_int.row(i2));
        } else {
          LOG(FATAL_THROW) << "Distance type not supported";
        }
      }
    }
  }

  return distances;
}

// Unit bearing vectors and their pixel Jacobians for a set of keypoints,
// together with a validity mask.
//
// Keypoints that cannot be unprojected - back-hemisphere pixels of an
// omnidirectional camera have no normalized image plane representation, and
// iterative undistortion can fail - are marked invalid and must be excluded
// from matching by the caller. Note that encoding invalidity as an extreme
// coordinate does *not* work: the Sampson error is a ratio whose numerator and
// denominator scale together, so a point pushed to infinity along a direction d
// converges to the finite distance between its partner and the epipolar line of
// d, which admits rather than rejects partners lying near that one line.
struct CamRaysWithJac {
  std::vector<Eigen::Vector3d> rays;
  std::vector<Eigen::Matrix3x2d> jacobians;
  std::vector<bool> valid;
};

CamRaysWithJac ComputeCamRaysWithJac(const Camera& camera,
                                     const FeatureKeypoints& keypoints) {
  CamRaysWithJac cam_rays;
  cam_rays.rays.resize(keypoints.size());
  cam_rays.jacobians.resize(keypoints.size());
  cam_rays.valid.resize(keypoints.size());
  for (size_t i = 0; i < keypoints.size(); ++i) {
    const FeatureKeypoint& keypoint = keypoints[i];
    if (const auto ray_and_jac = camera.CamRayFromImgWithJac(
            Eigen::Vector2d(keypoint.x, keypoint.y))) {
      cam_rays.rays[i] = ray_and_jac->ray;
      cam_rays.jacobians[i] = ray_and_jac->jacobian;
      cam_rays.valid[i] = true;
    } else {
      cam_rays.rays[i].setZero();
      cam_rays.jacobians[i].setZero();
      cam_rays.valid[i] = false;
    }
  }
  return cam_rays;
}

// Selects the epipolar model used to guide matching. The essential matrix is
// preferred whenever the intrinsics are known, which properly handles
// non-pinhole camera models where the fundamental matrix relationship does not
// hold. The intrinsics are known either from priors (calibrated configs) or
// because the two-view solver recovered them: such pairs are labeled
// UNCALIBRATED but carry the estimated intrinsics in `camera1`/`camera2`.
// Either side alone suffices, to support solvers that estimate only one side
// against an already calibrated view.
bool UseEssentialMatrixForGuidedMatching(const TwoViewGeometry& geometry) {
  if (!geometry.E.has_value()) {
    return false;
  }
  if (geometry.config == TwoViewGeometry::CALIBRATED ||
      geometry.config == TwoViewGeometry::CALIBRATED_RIG) {
    return true;
  }
  if (geometry.config == TwoViewGeometry::UNCALIBRATED) {
    return geometry.camera1.has_value() || geometry.camera2.has_value();
  }
  return false;
}

class SiftCPUFeatureMatcher : public FeatureMatcher {
 public:
  explicit SiftCPUFeatureMatcher(const FeatureMatchingOptions& options)
      : options_(options) {
    THROW_CHECK(options_.Check());
  }

  static std::unique_ptr<FeatureMatcher> Create(
      const FeatureMatchingOptions& options) {
    return std::make_unique<SiftCPUFeatureMatcher>(options);
  }

  void Match(const Image& image1,
             const Image& image2,
             FeatureMatches* matches) override {
    THROW_CHECK_NOTNULL(matches);
    ThrowCheckFeatureTypesMatch(image1, image2);

    matches->clear();

    std::shared_ptr<FeatureDescriptorIndex> index1;
    std::shared_ptr<FeatureDescriptorIndex> index2;
    if (!options_.sift->cpu_brute_force_matcher) {
      auto get_index = [this](const Image& image) {
        return options_.sift->cpu_descriptor_index_cache->Get(image.image_id);
      };
      index1 = index_cache_.GetOrCreate(image1, get_index);
      index2 = index_cache_.GetOrCreate(image2, get_index);
    }

    if (image1.descriptors->data.rows() == 0 ||
        image2.descriptors->data.rows() == 0) {
      return;
    }

    if (options_.sift->cpu_brute_force_matcher) {
      const Eigen::RowMajorMatrixXf dot_products =
          ComputeSiftDistanceMatrix(DistanceType::DOT_PRODUCT,
                                    image1.descriptors->data,
                                    image2.descriptors->data,
                                    nullptr);
      FindBestMatchesBruteForce(dot_products,
                                options_.sift->max_ratio,
                                options_.sift->max_distance,
                                options_.sift->cross_check,
                                matches);
      return;
    }

    Eigen::RowMajorMatrixXi indices_1to2;
    Eigen::RowMajorMatrixXf l2_dists_1to2;
    Eigen::RowMajorMatrixXi indices_2to1;
    Eigen::RowMajorMatrixXf l2_dists_2to1;

    THROW_CHECK_NOTNULL(index2)->Search(
        /*num_neighbors=*/2,
        image1.descriptors->ToFloat(),
        indices_1to2,
        l2_dists_1to2);
    if (options_.sift->cross_check) {
      THROW_CHECK_NOTNULL(index1)->Search(
          /*num_neighbors=*/2,
          image2.descriptors->ToFloat(),
          indices_2to1,
          l2_dists_2to1);
    }

    FindBestMatchesIndex(indices_1to2,
                         l2_dists_1to2,
                         indices_2to1,
                         l2_dists_2to1,
                         options_.sift->max_ratio,
                         options_.sift->max_distance,
                         options_.sift->cross_check,
                         matches);
  }

  void MatchGuided(const double max_error,
                   const Image& image1,
                   const Image& image2,
                   TwoViewGeometry* two_view_geometry) override {
    THROW_CHECK_NOTNULL(two_view_geometry);
    ThrowCheckFeatureTypesMatch(image1, image2, /*check_keypoints=*/true);

    two_view_geometry->inlier_matches.clear();

    const bool use_essential_matrix =
        UseEssentialMatrixForGuidedMatching(*two_view_geometry);
    const bool use_fundamental_matrix =
        !use_essential_matrix &&
        two_view_geometry->config == TwoViewGeometry::UNCALIBRATED &&
        two_view_geometry->F.has_value();
    const bool use_homography =
        (two_view_geometry->config == TwoViewGeometry::PLANAR ||
         two_view_geometry->config == TwoViewGeometry::PANORAMIC ||
         two_view_geometry->config == TwoViewGeometry::PLANAR_OR_PANORAMIC) &&
        two_view_geometry->H.has_value();
    // Normalize with the intrinsics the solver estimated where available (its
    // focal, not the camera's stale default), else the given cameras.
    const Camera& effective_camera1 = two_view_geometry->camera1.has_value()
                                          ? *two_view_geometry->camera1
                                          : *image1.camera;
    const Camera& effective_camera2 = two_view_geometry->camera2.has_value()
                                          ? *two_view_geometry->camera2
                                          : *image2.camera;
    // A spherical camera has no meaningful image plane, so a homography over
    // pixel coordinates is not a valid model for it. No estimator produces one
    // today; fail loudly rather than silently dividing by a near-zero z.
    if (use_homography) {
      THROW_CHECK(!effective_camera1.IsSpherical());
      THROW_CHECK(!effective_camera2.IsSpherical());
    }

    // The essential matrix path scores in pixels with the tangent Sampson
    // error, matching the two-view verification that produced E. Bearings are
    // used rather than normalized image plane coordinates so that the filter is
    // defined for every central camera model, including omnidirectional ones
    // whose back hemisphere has no image plane representation at all.
    const CamRaysWithJac cam_rays1 =
        use_essential_matrix
            ? ComputeCamRaysWithJac(effective_camera1, *image1.keypoints)
            : CamRaysWithJac();
    const CamRaysWithJac cam_rays2 =
        use_essential_matrix
            ? ComputeCamRaysWithJac(effective_camera2, *image2.keypoints)
            : CamRaysWithJac();

    const Eigen::Matrix3d E =
        use_essential_matrix ? *two_view_geometry->E : Eigen::Matrix3d::Zero();
    const Eigen::Matrix3f F =
        use_fundamental_matrix
            ? Eigen::Matrix3f(two_view_geometry->F->cast<float>())
            : Eigen::Matrix3f::Zero();
    const Eigen::Matrix3f H =
        use_homography ? Eigen::Matrix3f(two_view_geometry->H->cast<float>())
                       : Eigen::Matrix3f::Zero();

    // Both thresholds must outlive the lambdas below, which capture by
    // reference and are invoked after this scope's inner blocks have exited.
    const double max_residual_double = max_error * max_error;
    const float max_residual = static_cast<float>(max_residual_double);

    std::function<bool(Eigen::Index, Eigen::Index)> guided_filter;
    if (use_essential_matrix) {
      guided_filter = [&](const Eigen::Index i1, const Eigen::Index i2) {
        if (!cam_rays1.valid[i1] || !cam_rays2.valid[i2]) {
          return true;
        }
        return ComputeSquaredTangentSampsonError(cam_rays1.rays[i1],
                                                 cam_rays1.jacobians[i1],
                                                 cam_rays2.rays[i2],
                                                 cam_rays2.jacobians[i2],
                                                 E) > max_residual_double;
      };
    } else if (use_fundamental_matrix) {
      guided_filter = [&](const Eigen::Index i1, const Eigen::Index i2) {
        const auto& keypoint1 = (*image1.keypoints)[i1];
        const auto& keypoint2 = (*image2.keypoints)[i2];
        const Eigen::Vector3f p1(keypoint1.x, keypoint1.y, 1.0f);
        const Eigen::Vector3f p2(keypoint2.x, keypoint2.y, 1.0f);
        const Eigen::Vector3f epipolar_line1 = F * p1;
        const Eigen::Vector3f epipolar_line2 = F.transpose() * p2;
        const float nom = p2.transpose() * epipolar_line1;
        const float denom_sq = epipolar_line1(0) * epipolar_line1(0) +
                               epipolar_line1(1) * epipolar_line1(1) +
                               epipolar_line2(0) * epipolar_line2(0) +
                               epipolar_line2(1) * epipolar_line2(1);
        return nom * nom > max_residual * denom_sq;
      };
    } else if (use_homography) {
      guided_filter = [&](const Eigen::Index i1, const Eigen::Index i2) {
        const auto& keypoint1 = (*image1.keypoints)[i1];
        const auto& keypoint2 = (*image2.keypoints)[i2];
        const Eigen::Vector3f p1(keypoint1.x, keypoint1.y, 1.0f);
        const Eigen::Vector2f p2(keypoint2.x, keypoint2.y);
        return ((H * p1).hnormalized() - p2).squaredNorm() > max_residual;
      };
    } else {
      return;
    }

    THROW_CHECK(guided_filter);
    // The guided filter indexes per-feature geometry (bearings with Jacobians,
    // or the keypoints themselves) by descriptor row, so the two must align.
    THROW_CHECK_EQ(image1.keypoints->size(), image1.descriptors->data.rows());
    THROW_CHECK_EQ(image2.keypoints->size(), image2.descriptors->data.rows());

    const Eigen::RowMajorMatrixXf l2_dists_1to2 =
        ComputeSiftDistanceMatrix(DistanceType::L2,
                                  image1.descriptors->data,
                                  image2.descriptors->data,
                                  guided_filter);
    const Eigen::RowMajorMatrixXf l2_dists_2to1 = l2_dists_1to2.transpose();

    Eigen::RowMajorMatrixXi indices_1to2(l2_dists_1to2.rows(),
                                         l2_dists_1to2.cols());
    for (int i = 0; i < indices_1to2.rows(); ++i) {
      indices_1to2.row(i) = Eigen::VectorXi::LinSpaced(
          indices_1to2.cols(), 0, indices_1to2.cols() - 1);
    }
    Eigen::RowMajorMatrixXi indices_2to1(l2_dists_1to2.cols(),
                                         l2_dists_1to2.rows());
    for (int i = 0; i < indices_2to1.rows(); ++i) {
      indices_2to1.row(i) = Eigen::VectorXi::LinSpaced(
          indices_2to1.cols(), 0, indices_2to1.cols() - 1);
    }

    FindBestMatchesIndex(indices_1to2,
                         l2_dists_1to2,
                         indices_2to1,
                         l2_dists_2to1,
                         options_.sift->max_ratio,
                         options_.sift->max_distance,
                         options_.sift->cross_check,
                         &two_view_geometry->inlier_matches);
  }

 private:
  const FeatureMatchingOptions options_;
  // Cache descriptor indexes per image to avoid redundant index lookups.
  ImageFeatureCache<std::shared_ptr<FeatureDescriptorIndex>> index_cache_;
};


class SiftOpenGLFeatureMatcher : public FeatureMatcher {
 public:
  explicit SiftOpenGLFeatureMatcher(const FeatureMatchingOptions& options)
      : options_(options), cpu_fallback_( MakeCpuOpts(options) ) {}

  static FeatureMatchingOptions MakeCpuOpts(FeatureMatchingOptions o) {
    o.use_gpu = false;
    o.sift->cpu_brute_force_matcher = true;
    return o;
  }

  static std::unique_ptr<FeatureMatcher> Create(
      const FeatureMatchingOptions& options) {
    auto m = std::make_unique<SiftOpenGLFeatureMatcher>(options);
    m->gl_engine_.Initialize();
    return m;
  }

  void Match(const Image& image1,
             const Image& image2,
             FeatureMatches* matches) override {
    THROW_CHECK_NOTNULL(matches);
    ThrowCheckFeatureTypesMatch(image1, image2);
    if (gl_engine_.IsAvailable()) {
      if (gl_engine_.Match(image1.descriptors->data,
                           image2.descriptors->data,
                           static_cast<float>(options_.sift->max_ratio),
                           static_cast<float>(options_.sift->max_distance),
                           options_.sift->cross_check,
                           matches)) {
        return;
      }
    }
    cpu_fallback_.Match(image1, image2, matches);
  }

  void MatchGuided(double max_error,
                   const Image& image1,
                   const Image& image2,
                   TwoViewGeometry* two_view_geometry) override {
    cpu_fallback_.MatchGuided(max_error, image1, image2, two_view_geometry);
  }

 private:
  const FeatureMatchingOptions options_;
  OpenGLSiftEngine gl_engine_;
  SiftCPUFeatureMatcher cpu_fallback_;
};

}  // namespace

std::unique_ptr<FeatureMatcher> CreateSiftFeatureMatcher(
    const FeatureMatchingOptions& options) {
  if (options.use_gpu) {
    return SiftOpenGLFeatureMatcher::Create(options);
  }
  FeatureMatchingOptions cpu_opts = options;
  if (cpu_opts.sift->cpu_descriptor_index_cache == nullptr) {
    cpu_opts.sift->cpu_brute_force_matcher = true;
  }
  return SiftCPUFeatureMatcher::Create(cpu_opts);
}

void LoadSiftFeaturesFromTextFile(const std::filesystem::path&,
                                  FeatureKeypoints* keypoints,
                                  FeatureDescriptors* descriptors) {
  if (keypoints) keypoints->clear();
  if (descriptors) descriptors->data.resize(0, kSiftDescriptorDim);
}

}  // namespace colmap
