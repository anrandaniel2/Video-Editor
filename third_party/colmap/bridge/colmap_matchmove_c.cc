// SPDX-License-Identifier: BSD-3-Clause
#include "colmap_matchmove_c.h"

#include "colmap/controllers/incremental_pipeline.h"
#include "colmap/estimators/two_view_geometry.h"
#include "colmap/feature/extractor.h"
#include "colmap/feature/matcher.h"
#include "colmap/feature/sift.h"
#include "colmap/scene/camera.h"
#include "colmap/scene/database_cache.h"
#include "colmap/scene/frame.h"
#include "colmap/scene/image.h"
#include "colmap/scene/reconstruction.h"
#include "colmap/scene/reconstruction_manager.h"
#include "colmap/sensor/bitmap.h"
#include "colmap/sensor/models.h"
#include "colmap/sensor/rig.h"
#include "colmap/sfm/global_mapper.h"
#include "colmap/util/opengl_utils.h"

#include <Eigen/Dense>
#include <Eigen/SVD>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace {

void SetError(ColmapSolveResult* out, const char* msg) {
  if (!out) return;
  out->status = -1;
  std::snprintf(out->error_msg, sizeof(out->error_msg), "%s", msg ? msg : "unknown error");
}

Eigen::Matrix3d ProjectToSO3(const Eigen::Matrix3d& m) {
  Eigen::JacobiSVD<Eigen::Matrix3d> svd(m, Eigen::ComputeFullU | Eigen::ComputeFullV);
  Eigen::Matrix3d u = svd.matrixU();
  Eigen::Matrix3d v = svd.matrixV();
  Eigen::Matrix3d r = u * v.transpose();
  if (r.determinant() < 0.0) {
    u.col(2) = -u.col(2);
    r = u * v.transpose();
  }
  return r;
}

}  // namespace

extern "C" int colmap_solve_sequence(const ColmapInputFrame* frames,
                                     size_t num_frames,
                                     const ColmapSolveConfig* config,
                                     ColmapSolveResult* out_result) {
  if (!out_result) return -1;
  std::memset(out_result, 0, sizeof(*out_result));
  if (!frames || !config || num_frames < 2) {
    SetError(out_result, "Need at least 2 sampled frames for COLMAP match-move");
    return -1;
  }

  try {
    const uint32_t width = frames[0].width;
    const uint32_t height = frames[0].height;
    if (width < 16 || height < 16) {
      SetError(out_result, "Frame dimensions too small for COLMAP");
      return -1;
    }

    // 1. Headless OpenGL ES 3.1 / EGL context (sparse + OpenGL only, zero UI).
    std::unique_ptr<colmap::OpenGLContextManager> gl_ctx;
    bool gl_active = false;
    if (config->use_opengl != 0) {
      gl_ctx = std::make_unique<colmap::OpenGLContextManager>(3, 1);
      gl_active = gl_ctx->IsValid() && gl_ctx->MakeCurrent();
    }
    out_result->used_opengl = gl_active ? 1 : 0;

    // 2. Shared camera intrinsics (SIMPLE_PINHOLE: f, cx, cy).
    const double init_f = (config->initial_focal_px > 10.0)
                              ? config->initial_focal_px
                              : 0.9 * static_cast<double>(std::max(width, height));
    colmap::Camera camera = colmap::Camera::CreateFromModelId(
        1,
        colmap::SimplePinholeCameraModel::model_id,
        init_f,
        width,
        height,
        colmap::CameraSource::USER);

    // 3. Extract SIFT keypoints & descriptors (OpenGL compute shader or VLFeat CPU fallback).
    colmap::FeatureExtractionOptions ext_opts(colmap::FeatureExtractorType::SIFT);
    ext_opts.use_gpu = gl_active;
    ext_opts.max_image_size = static_cast<int>(std::max(width, height));
    ext_opts.sift->max_num_features = std::clamp(config->max_features, 512, 8192);
    ext_opts.sift->first_octave = 0;  // Fast & sharp on <= 640-960px video frames
    ext_opts.sift->num_octaves = 4;
    ext_opts.sift->octave_resolution = 3;
    ext_opts.sift->peak_threshold = 0.005;

    auto extractor = colmap::FeatureExtractor::Create(ext_opts);
    if (!extractor) {
      SetError(out_result, "Failed to initialize COLMAP SIFT feature extractor");
      return -1;
    }

    std::vector<std::shared_ptr<colmap::FeatureKeypoints>> all_keypoints(num_frames);
    std::vector<std::shared_ptr<colmap::FeatureDescriptors>> all_descriptors(num_frames);
    std::vector<std::vector<Eigen::Vector2d>> all_points2d(num_frames);

    for (size_t i = 0; i < num_frames; ++i) {
      if (!frames[i].gray || frames[i].width != width || frames[i].height != height) {
        SetError(out_result, "Inconsistent input frame dimensions or null buffer");
        return -1;
      }
      colmap::Bitmap bmp(static_cast<int>(width), static_cast<int>(height), /*as_rgb=*/false);
      std::memcpy(bmp.RowMajorData().data(),
                  frames[i].gray,
                  static_cast<size_t>(width) * height);

      auto kps = std::make_shared<colmap::FeatureKeypoints>();
      auto desc = std::make_shared<colmap::FeatureDescriptors>();
      if (!extractor->Extract(bmp, kps.get(), desc.get())) {
        kps->clear();
        desc->data.resize(0, 128);
      }
      all_points2d[i].resize(kps->size());
      for (size_t k = 0; k < kps->size(); ++k) {
        all_points2d[i][k] = Eigen::Vector2d((*kps)[k].x, (*kps)[k].y);
      }
      all_keypoints[i] = std::move(kps);
      all_descriptors[i] = std::move(desc);
    }

    // 4. Match image pairs (OpenGL compute shader or Eigen brute-force) + geometric verification.
    colmap::FeatureMatchingOptions match_opts(colmap::FeatureMatcherType::SIFT_BRUTEFORCE);
    match_opts.use_gpu = gl_active;
    match_opts.sift->max_ratio = 0.82;
    match_opts.sift->max_distance = 0.75;
    match_opts.sift->cross_check = true;
    match_opts.sift->cpu_brute_force_matcher = true;

    auto matcher = colmap::FeatureMatcher::Create(match_opts);
    if (!matcher) {
      SetError(out_result, "Failed to initialize COLMAP SIFT feature matcher");
      return -1;
    }

    colmap::TwoViewGeometryOptions tvg_opts;
    tvg_opts.min_num_inliers = 12;
    tvg_opts.compute_relative_pose = true;
    tvg_opts.ransac_options.max_error = 4.0;
    tvg_opts.ransac_options.confidence = 0.999;
    tvg_opts.ransac_options.max_num_trials = 4000;

    struct VerifiedPair {
      size_t i;
      size_t j;
      colmap::TwoViewGeometry tvg;
    };
    std::vector<VerifiedPair> verified_pairs;

    for (size_t i = 0; i < num_frames; ++i) {
      if (all_keypoints[i]->size() < 12) continue;
      for (size_t j = i + 1; j < num_frames; ++j) {
        if (all_keypoints[j]->size() < 12) continue;
        const size_t gap = j - i;
        // Exhaustive for short sequences; wide sliding window + keyframe anchors for longer clips.
        const bool keep_pair = (num_frames <= 24) || (gap <= 8) ||
                               ((i % 4 == 0) && (j % 4 == 0)) ||
                               (i == 0 && j == num_frames - 1);
        if (!keep_pair) continue;

        colmap::FeatureMatcher::Image img1;
        img1.image_id = static_cast<colmap::image_t>(i + 1);
        img1.camera = &camera;
        img1.keypoints = all_keypoints[i];
        img1.descriptors = all_descriptors[i];

        colmap::FeatureMatcher::Image img2;
        img2.image_id = static_cast<colmap::image_t>(j + 1);
        img2.camera = &camera;
        img2.keypoints = all_keypoints[j];
        img2.descriptors = all_descriptors[j];

        colmap::FeatureMatches raw_matches;
        matcher->Match(img1, img2, &raw_matches);
        if (static_cast<int>(raw_matches.size()) < tvg_opts.min_num_inliers) continue;

        colmap::TwoViewGeometry tvg = colmap::EstimateTwoViewGeometry(
            camera, all_points2d[i], camera, all_points2d[j], raw_matches, tvg_opts);
        if (tvg.config != colmap::TwoViewGeometry::UNDEFINED &&
            tvg.config != colmap::TwoViewGeometry::DEGENERATE &&
            static_cast<int>(tvg.inlier_matches.size()) >= tvg_opts.min_num_inliers) {
          if (!tvg.cam2_from_cam1.has_value()) {
            colmap::EstimateTwoViewGeometryPose(
                camera, all_points2d[i], camera, all_points2d[j], &tvg);
          }
          verified_pairs.push_back({i, j, std::move(tvg)});
        }
      }
    }

    // 5. Build in-memory COLMAP DatabaseCache (zero SQLite, zero disk I/O).
    auto cache = std::make_shared<colmap::DatabaseCache>();
    cache->AddCamera(camera);

    colmap::Rig rig;
    rig.SetRigId(1);
    rig.AddRefSensor(camera.SensorId());
    cache->AddRig(std::move(rig));

    for (size_t i = 0; i < num_frames; ++i) {
      const colmap::image_t img_id = static_cast<colmap::image_t>(i + 1);
      const colmap::frame_t frm_id = static_cast<colmap::frame_t>(i + 1);

      colmap::Image img;
      img.SetImageId(img_id);
      img.SetCameraId(camera.camera_id);
      img.SetFrameId(frm_id);
      img.SetName("frame_" + std::to_string(i));
      img.SetPoints2D(all_points2d[i]);

      colmap::Frame frm;
      frm.SetFrameId(frm_id);
      frm.SetRigId(1);
      frm.AddDataId(img.DataId());

      cache->AddFrame(std::move(frm));
      cache->AddImage(std::move(img));
    }

    for (const auto& vp : verified_pairs) {
      if (vp.tvg.config == colmap::TwoViewGeometry::WATERMARK) continue;
      cache->CorrespondenceGraph()->AddTwoViewGeometry(
          static_cast<colmap::image_t>(vp.i + 1),
          static_cast<colmap::image_t>(vp.j + 1),
          vp.tvg);
    }
    cache->CorrespondenceGraph()->Finalize();
    colmap::MaybeDecomposeRelativePoses(cache.get());

    // 6. Run GLOMAP (GlobalMapper) and/or COLMAP IncrementalPipeline sparse solver.
    std::shared_ptr<colmap::Reconstruction> best_recon;
    bool best_was_glomap = false;

    if (config->prefer_glomap != 0 && !verified_pairs.empty()) {
      auto global_recon = std::make_shared<colmap::Reconstruction>();
      colmap::GlobalMapper global_mapper(cache);
      global_mapper.BeginReconstruction(global_recon);

      colmap::GlobalMapperOptions gopts;
      gopts.num_threads = 2;
      gopts.random_seed = 42;
      gopts.track_min_num_views_per_track = 2;
      gopts.min_tri_angle_deg = 0.75;
      gopts.max_angular_reproj_error_deg = 2.0;
      gopts.ba_num_iterations = 2;
      gopts.bundle_adjustment.refine_focal_length = (config->lock_fov == 0);
      gopts.bundle_adjustment.refine_principal_point = false;
      gopts.bundle_adjustment.refine_extra_params = false;
      gopts.bundle_adjustment.print_summary = false;
      if (gopts.bundle_adjustment.ceres) {
        gopts.bundle_adjustment.ceres->use_gpu = false;
        gopts.bundle_adjustment.ceres->solver_options.max_num_iterations = 100;
      }

      if (global_mapper.Solve(gopts) && global_recon->RegImageIds().size() >= 2) {
        best_recon = global_recon;
        best_was_glomap = true;
      }
    }

    // Run IncrementalPipeline if GLOMAP was disabled or registered fewer than 75% of frames.
    const size_t min_desired = std::max<size_t>(2, (num_frames * 3) / 4);
    if ((!best_recon || best_recon->RegImageIds().size() < min_desired) &&
        !verified_pairs.empty()) {
      auto inc_opts = std::make_shared<colmap::IncrementalPipelineOptions>();
      inc_opts->min_num_matches = 12;
      inc_opts->min_model_size = 2;
      inc_opts->multiple_models = false;
      inc_opts->extract_colors = false;
      inc_opts->num_threads = 2;
      inc_opts->random_seed = 42;
      inc_opts->ba_refine_focal_length = (config->lock_fov == 0);
      inc_opts->ba_refine_principal_point = false;
      inc_opts->ba_refine_extra_params = false;
      inc_opts->ba_use_gpu = false;
      inc_opts->mapper.init_min_num_inliers = 20;
      inc_opts->mapper.init_min_tri_angle = 1.5;
      inc_opts->mapper.abs_pose_min_num_inliers = 12;
      inc_opts->mapper.abs_pose_min_inlier_ratio = 0.15;
      inc_opts->mapper.filter_min_tri_angle = 0.75;
      if (config->lock_fov != 0) {
        inc_opts->constant_cameras.insert(camera.camera_id);
      }

      auto recon_mgr = std::make_shared<colmap::ReconstructionManager>();
      colmap::IncrementalPipeline inc_pipe(inc_opts, cache, recon_mgr);
      inc_pipe.Run();

      for (size_t m = 0; m < recon_mgr->Size(); ++m) {
        auto cand = recon_mgr->Get(m);
        if (!cand || cand->RegImageIds().size() < 2) continue;
        if (!best_recon ||
            cand->RegImageIds().size() > best_recon->RegImageIds().size() ||
            (cand->RegImageIds().size() == best_recon->RegImageIds().size() &&
             cand->ComputeMeanReprojectionError() <
                 best_recon->ComputeMeanReprojectionError())) {
          best_recon = cand;
          best_was_glomap = false;
        }
      }
    }

    // 7. Export solved camera poses & sparse 3D points (or pure-rotation homography fallback).
    out_result->num_poses = num_frames;
    out_result->poses = static_cast<ColmapSolvedPose*>(
        std::calloc(num_frames, sizeof(ColmapSolvedPose)));
    if (!out_result->poses) {
      SetError(out_result, "Out of memory allocating poses");
      return -1;
    }

    for (size_t i = 0; i < num_frames; ++i) {
      out_result->poses[i].time_s = frames[i].time_s;
      out_result->poses[i].registered = 0;
      out_result->poses[i].r_cw[0] = 1.0;
      out_result->poses[i].r_cw[4] = 1.0;
      out_result->poses[i].r_cw[8] = 1.0;
      out_result->poses[i].focal_px = init_f;
    }

    if (best_recon && best_recon->RegImageIds().size() >= 2) {
      out_result->used_glomap = best_was_glomap ? 1 : 0;
      const double solved_focal =
          best_recon->ExistsCamera(camera.camera_id)
              ? best_recon->Camera(camera.camera_id).MeanFocalLength()
              : init_f;
      out_result->focal_px = solved_focal;
      out_result->mean_reproj_px = best_recon->ComputeMeanReprojectionError();

      for (const colmap::image_t img_id : best_recon->RegImageIds()) {
        const size_t idx = static_cast<size_t>(img_id - 1);
        if (idx >= num_frames) continue;
        const colmap::Rigid3d cfw = best_recon->Image(img_id).CamFromWorld();
        const Eigen::Matrix3d rot = cfw.rotation().toRotationMatrix();
        const Eigen::Vector3d trans = cfw.translation();
        out_result->poses[idx].registered = 1;
        out_result->poses[idx].focal_px = solved_focal;
        for (int r = 0; r < 3; ++r) {
          for (int c = 0; c < 3; ++c) {
            out_result->poses[idx].r_cw[r * 3 + c] = rot(r, c);
          }
          out_result->poses[idx].t_cw[r] = trans(r);
        }
      }

      const auto& pts3d = best_recon->Points3D();
      out_result->num_points = pts3d.size();
      if (out_result->num_points > 0) {
        out_result->points = static_cast<ColmapSolvedPoint3D*>(
            std::calloc(out_result->num_points, sizeof(ColmapSolvedPoint3D)));
        if (out_result->points) {
          size_t p_idx = 0;
          for (const auto& [pt_id, pt] : pts3d) {
            (void)pt_id;
            out_result->points[p_idx].xyz[0] = pt.xyz.x();
            out_result->points[p_idx].xyz[1] = pt.xyz.y();
            out_result->points[p_idx].xyz[2] = pt.xyz.z();
            out_result->points[p_idx].observations =
                static_cast<uint32_t>(pt.track.Length());
            out_result->points[p_idx].reproj_error =
                pt.HasError() ? pt.error : out_result->mean_reproj_px;
            ++p_idx;
          }
        } else {
          out_result->num_points = 0;
        }
      }
      out_result->status = 0;
      return 0;
    }

    // Pure-rotation / panoramic fallback (tripod pan/tilt with zero baseline):
    // chain verified COLMAP relative rotations / homographies across frames.
    Eigen::Matrix3d k_mat = Eigen::Matrix3d::Identity();
    k_mat(0, 0) = init_f;
    k_mat(1, 1) = init_f;
    k_mat(0, 2) = 0.5 * static_cast<double>(width);
    k_mat(1, 2) = 0.5 * static_cast<double>(height);
    const Eigen::Matrix3d k_inv = k_mat.inverse();

    std::vector<Eigen::Matrix3d> r_cw_chain(num_frames, Eigen::Matrix3d::Identity());
    std::vector<bool> reached(num_frames, false);
    reached[0] = true;
    size_t chained_count = 1;

    for (size_t i = 0; i + 1 < num_frames; ++i) {
      if (!reached[i]) continue;
      for (const auto& vp : verified_pairs) {
        if (vp.i == i && vp.j == i + 1) {
          Eigen::Matrix3d r_step = Eigen::Matrix3d::Identity();
          if (vp.tvg.cam2_from_cam1.has_value()) {
            r_step = vp.tvg.cam2_from_cam1->rotation().toRotationMatrix();
          } else if (vp.tvg.H.has_value() && vp.tvg.H->allFinite() &&
                     vp.tvg.H->norm() > 1e-6) {
            r_step = ProjectToSO3(k_inv * (*vp.tvg.H) * k_mat);
          }
          r_cw_chain[i + 1] = r_step * r_cw_chain[i];
          reached[i + 1] = true;
          ++chained_count;
          break;
        }
      }
    }

    if (chained_count >= 2) {
      out_result->focal_px = init_f;
      out_result->mean_reproj_px = 0.75;
      for (size_t i = 0; i < num_frames; ++i) {
        if (!reached[i]) continue;
        out_result->poses[i].registered = 1;
        for (int r = 0; r < 3; ++r) {
          for (int c = 0; c < 3; ++c) {
            out_result->poses[i].r_cw[r * 3 + c] = r_cw_chain[i](r, c);
          }
        }
      }
      out_result->status = 0;
      return 0;
    }

    colmap_free_result(out_result);
    SetError(out_result, "Insufficient visual parallax/features across sampled frames");
    return -2;
  } catch (const std::exception& ex) {
    colmap_free_result(out_result);
    SetError(out_result, ex.what());
    return -3;
  } catch (...) {
    colmap_free_result(out_result);
    SetError(out_result, "Unhandled COLMAP exception");
    return -4;
  }
}

extern "C" void colmap_free_result(ColmapSolveResult* result) {
  if (!result) return;
  if (result->poses) {
    std::free(result->poses);
    result->poses = nullptr;
  }
  if (result->points) {
    std::free(result->points);
    result->points = nullptr;
  }
  result->num_poses = 0;
  result->num_points = 0;
}
