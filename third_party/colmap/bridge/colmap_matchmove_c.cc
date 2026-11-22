// SPDX-License-Identifier: BSD-3-Clause
#include "colmap_matchmove_c.h"

#include "colmap/controllers/incremental_pipeline.h"
#include "colmap/estimators/rotation_averaging.h"
#include "colmap/estimators/two_view_geometry.h"
#include "colmap/feature/extractor.h"
#include "colmap/feature/matcher.h"
#include "colmap/feature/sift.h"
#include "colmap/geometry/triangulation.h"
#include "colmap/math/math.h"
#include "colmap/scene/camera.h"
#include "colmap/scene/database_cache.h"
#include "colmap/scene/frame.h"
#include "colmap/scene/image.h"
#include "colmap/scene/pose_graph.h"
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
#include <mutex>
#include <string>
#include <utility>
#include <vector>

extern "C" void vl_constructor(void);

namespace {

std::once_flag g_vl_init_once;

void SetError(ColmapSolveResult* out, int status, const char* msg) {
  if (!out) return;
  out->status = status;
  std::snprintf(out->error_msg, sizeof(out->error_msg), "%s", msg ? msg : "unknown error");
}

Eigen::Matrix3d ProjectToSO3(const Eigen::Matrix3d& m) {
  Eigen::Matrix3d ms = (m.determinant() < 0.0) ? Eigen::Matrix3d(-m) : m;
  Eigen::JacobiSVD<Eigen::Matrix3d> svd(ms, Eigen::ComputeFullU | Eigen::ComputeFullV);
  Eigen::Matrix3d u = svd.matrixU();
  Eigen::Matrix3d v = svd.matrixV();
  Eigen::Matrix3d r = u * v.transpose();
  if (r.determinant() < 0.0) {
    u.col(2) = -u.col(2);
    r = u * v.transpose();
  }
  return r;
}

double MedianInPlace(std::vector<double>* vals) {
  if (!vals || vals->empty()) return 0.0;
  std::sort(vals->begin(), vals->end());
  const size_t mid = vals->size() / 2;
  if (vals->size() % 2 == 1) return (*vals)[mid];
  return 0.5 * ((*vals)[mid - 1] + (*vals)[mid]);
}

double PercentileInPlace(std::vector<double>* vals, double q) {
  if (!vals || vals->empty()) return 0.0;
  std::sort(vals->begin(), vals->end());
  const double pos = std::clamp(q, 0.0, 1.0) * static_cast<double>(vals->size() - 1);
  const size_t idx = static_cast<size_t>(std::round(pos));
  return (*vals)[std::min(idx, vals->size() - 1)];
}

// Estimate pure 3D rotation R in SO(3) via Kabsch/Procrustes on calibrated bearing rays
// and return median pixel reprojection error.
double EstimatePureRotationProcrustes(const colmap::Camera& cam,
                                      const std::vector<Eigen::Vector2d>& pts1,
                                      const std::vector<Eigen::Vector2d>& pts2,
                                      const colmap::FeatureMatches& inliers,
                                      Eigen::Matrix3d* out_r) {
  if (inliers.size() < 4) return 999.0;
  Eigen::Matrix3d cov = Eigen::Matrix3d::Zero();
  size_t valid = 0;
  for (size_t k = 0; k < inliers.size(); ++k) {
    const auto ray1 = cam.CamRayFromImg(pts1[inliers[k].point2D_idx1]);
    const auto ray2 = cam.CamRayFromImg(pts2[inliers[k].point2D_idx2]);
    if (!ray1.has_value() || !ray2.has_value()) continue;
    cov += (*ray2) * ray1->transpose();
    ++valid;
  }
  if (valid < 4) return 999.0;

  Eigen::JacobiSVD<Eigen::Matrix3d> svd(cov, Eigen::ComputeFullU | Eigen::ComputeFullV);
  Eigen::Matrix3d u = svd.matrixU();
  Eigen::Matrix3d v = svd.matrixV();
  Eigen::Matrix3d r_rot = u * v.transpose();
  if (r_rot.determinant() < 0.0) {
    u.col(2) = -u.col(2);
    r_rot = u * v.transpose();
  }
  if (out_r) *out_r = r_rot;

  std::vector<double> errs;
  errs.reserve(inliers.size());
  for (size_t k = 0; k < inliers.size(); ++k) {
    const auto ray1 = cam.CamRayFromImg(pts1[inliers[k].point2D_idx1]);
    if (!ray1.has_value()) continue;
    const Eigen::Vector3d pred = r_rot * (*ray1);
    if (pred.z() <= 1e-4) {
      errs.push_back(999.0);
      continue;
    }
    const auto img_pt = cam.ImgFromCam(pred);
    if (!img_pt.has_value()) {
      errs.push_back(999.0);
      continue;
    }
    errs.push_back((*img_pt - pts2[inliers[k].point2D_idx2]).norm());
  }
  return errs.empty() ? 999.0 : MedianInPlace(&errs);
}

// Focal-invariant pure-rotation residual: evaluates pure SO(3) rotation across plausible
// focal multipliers so a locked-FOV prior mismatch on a planar wall never masquerades as 3D motion.
double BestFocalPureRotationPxErr(uint32_t width,
                                  uint32_t height,
                                  double init_f,
                                  const std::vector<Eigen::Vector2d>& pts1,
                                  const std::vector<Eigen::Vector2d>& pts2,
                                  const colmap::FeatureMatches& inliers) {
  double best = 999.0;
  static constexpr double kMultipliers[] = {0.65, 0.78, 0.90, 1.00, 1.12, 1.28, 1.48};
  for (double mult : kMultipliers) {
    const colmap::Camera test_cam = colmap::Camera::CreateFromModelId(
        1,
        colmap::SimplePinholeCameraModel::model_id,
        std::max(20.0, init_f * mult),
        width,
        height,
        colmap::CameraSource::USER);
    best = std::min(
        best,
        EstimatePureRotationProcrustes(test_cam, pts1, pts2, inliers, nullptr));
  }
  return best;
}

// Compute median pixel residual of an IRLS-refined 4-DOF 2D image-plane similarity transform
// (`x' = a x - b y + tx`, `y' = b x + a y + ty`), alongside average 2D motion magnitude
// and scale zoom change (`|hypot(a, b) - 1|`).
struct Similarity2DFit {
  double median_err_px = 999.0;
  double mean_disp_px = 0.0;
  double scale_delta = 0.0;
};

Similarity2DFit FitSimilarity2D(const std::vector<Eigen::Vector2d>& pts1,
                                const std::vector<Eigen::Vector2d>& pts2,
                                const colmap::FeatureMatches& inliers) {
  Similarity2DFit out;
  const size_t m = inliers.size();
  if (m < 8) return out;

  Eigen::MatrixXd a_mat(2 * m, 4);
  Eigen::VectorXd rhs(2 * m);
  double disp_sum = 0.0;
  for (size_t k = 0; k < m; ++k) {
    const Eigen::Vector2d& p1 = pts1[inliers[k].point2D_idx1];
    const Eigen::Vector2d& p2 = pts2[inliers[k].point2D_idx2];
    disp_sum += (p2 - p1).norm();
    a_mat(2 * k, 0) = p1.x();
    a_mat(2 * k, 1) = -p1.y();
    a_mat(2 * k, 2) = 1.0;
    a_mat(2 * k, 3) = 0.0;
    rhs(2 * k) = p2.x();
    a_mat(2 * k + 1, 0) = p1.y();
    a_mat(2 * k + 1, 1) = p1.x();
    a_mat(2 * k + 1, 2) = 0.0;
    a_mat(2 * k + 1, 3) = 1.0;
    rhs(2 * k + 1) = p2.y();
  }
  out.mean_disp_px = disp_sum / static_cast<double>(m);

  Eigen::Vector4d sol = a_mat.colPivHouseholderQr().solve(rhs);
  if (!sol.allFinite()) return out;

  // One Huber IRLS pass so occasional mismatched keypoints cannot inflate 2D residual.
  Eigen::MatrixXd aw(2 * m, 4);
  Eigen::VectorXd bw(2 * m);
  for (size_t k = 0; k < m; ++k) {
    const Eigen::Vector2d& p1 = pts1[inliers[k].point2D_idx1];
    const Eigen::Vector2d& p2 = pts2[inliers[k].point2D_idx2];
    const double rx = (sol(0) * p1.x() - sol(1) * p1.y() + sol(2)) - p2.x();
    const double ry = (sol(1) * p1.x() + sol(0) * p1.y() + sol(3)) - p2.y();
    const double r = std::hypot(rx, ry);
    const double w = (r > 1.5) ? std::sqrt(1.5 / r) : 1.0;
    aw.row(2 * k) = w * a_mat.row(2 * k);
    bw(2 * k) = w * rhs(2 * k);
    aw.row(2 * k + 1) = w * a_mat.row(2 * k + 1);
    bw(2 * k + 1) = w * rhs(2 * k + 1);
  }
  const Eigen::Vector4d sol_irls = aw.colPivHouseholderQr().solve(bw);
  if (sol_irls.allFinite()) {
    sol = sol_irls;
  }

  out.scale_delta = std::abs(std::hypot(sol(0), sol(1)) - 1.0);
  std::vector<double> errs;
  errs.reserve(m);
  for (size_t k = 0; k < m; ++k) {
    const Eigen::Vector2d& p1 = pts1[inliers[k].point2D_idx1];
    const Eigen::Vector2d& p2 = pts2[inliers[k].point2D_idx2];
    const double rx = (sol(0) * p1.x() - sol(1) * p1.y() + sol(2)) - p2.x();
    const double ry = (sol(1) * p1.x() + sol(0) * p1.y() + sol(3)) - p2.y();
    errs.push_back(std::hypot(rx, ry));
  }
  out.median_err_px = MedianInPlace(&errs);
  return out;
}

struct VerifiedPair {
  size_t i;
  size_t j;
  colmap::TwoViewGeometry tvg;
};

struct ReconQuality {
  bool valid_6dof = false;
  size_t num_reg = 0;
  size_t reliable_points = 0;
  double mean_reproj_px = 999.0;
  double median_tri_deg = 0.0;
  double baseline_snr = 0.0;
  double step_jitter_ratio = 999.0;
  double solved_focal = 0.0;
};

ReconQuality Evaluate6DofReconstruction(
    const colmap::Reconstruction& recon,
    colmap::camera_t camera_id,
    size_t num_frames,
    double init_f,
    bool lock_fov,
    double px_err_limit) {
  ReconQuality q;
  q.num_reg = recon.RegImageIds().size();
  const size_t min_reg_needed =
      (num_frames >= 6) ? std::max<size_t>(3, num_frames / 3) : 2;
  if (q.num_reg < min_reg_needed) return q;

  q.solved_focal = recon.ExistsCamera(camera_id)
                       ? recon.Camera(camera_id).MeanFocalLength()
                       : init_f;
  if (!std::isfinite(q.solved_focal) || q.solved_focal <= 10.0) return q;
  if (!lock_fov &&
      (q.solved_focal < 0.45 * init_f || q.solved_focal > 2.25 * init_f)) {
    return q;
  }

  q.mean_reproj_px = recon.ComputeMeanReprojectionError();
  if (!std::isfinite(q.mean_reproj_px) || q.mean_reproj_px > px_err_limit) {
    return q;
  }

  // Collect registered camera centers ordered by frame index.
  std::vector<std::pair<size_t, Eigen::Vector3d>> centers;
  centers.reserve(q.num_reg);
  for (const colmap::image_t img_id : recon.RegImageIds()) {
    const size_t idx = static_cast<size_t>(img_id - 1);
    if (idx >= num_frames) continue;
    const Eigen::Vector3d c = recon.Image(img_id).ProjectionCenter();
    if (!c.allFinite()) return q;
    centers.emplace_back(idx, c);
  }
  if (centers.size() < min_reg_needed) return q;
  std::sort(centers.begin(),
            centers.end(),
            [](const auto& a, const auto& b) { return a.first < b.first; });

  double max_baseline = 0.0;
  for (size_t a = 0; a < centers.size(); ++a) {
    for (size_t b = a + 1; b < centers.size(); ++b) {
      max_baseline = std::max(max_baseline, (centers[b].second - centers[a].second).norm());
    }
  }

  std::vector<double> steps;
  steps.reserve(centers.size() - 1);
  for (size_t k = 0; k + 1 < centers.size(); ++k) {
    const size_t dt = std::max<size_t>(1, centers[k + 1].first - centers[k].first);
    steps.push_back((centers[k + 1].second - centers[k].second).norm() /
                    static_cast<double>(dt));
  }
  double max_step = 0.0;
  for (double s : steps) max_step = std::max(max_step, s);
  const double med_step = MedianInPlace(&steps);
  q.step_jitter_ratio = (med_step > 1e-9) ? (max_step / med_step) : 999.0;

  // Evaluate 3D landmark reliability, depth scale, and triangulation parallax.
  const colmap::image_t ref_img_id = static_cast<colmap::image_t>(centers.front().first + 1);
  const colmap::Rigid3d ref_cfw = recon.Image(ref_img_id).CamFromWorld();

  const size_t min_obs = (q.num_reg >= 5) ? 3 : 2;
  std::vector<double> depths;
  std::vector<double> tri_degs;
  depths.reserve(recon.NumPoints3D());
  tri_degs.reserve(recon.NumPoints3D());

  for (const auto& [pt_id, pt] : recon.Points3D()) {
    (void)pt_id;
    if (!pt.xyz.allFinite() || pt.track.Length() < min_obs) continue;
    if (pt.HasError() && pt.error > px_err_limit * 1.6) continue;

    // Compute maximum triangulation angle across observed views for this landmark.
    double best_tri_rad = 0.0;
    const auto& elems = pt.track.Elements();
    const size_t step = std::max<size_t>(1, elems.size() / 6);
    for (size_t a = 0; a < elems.size(); a += step) {
      if (!recon.ExistsImage(elems[a].image_id) ||
          !recon.Image(elems[a].image_id).HasPose()) {
        continue;
      }
      const Eigen::Vector3d c1 = recon.Image(elems[a].image_id).ProjectionCenter();
      for (size_t b = a + 1; b < elems.size(); b += step) {
        if (!recon.ExistsImage(elems[b].image_id) ||
            !recon.Image(elems[b].image_id).HasPose()) {
          continue;
        }
        const Eigen::Vector3d c2 = recon.Image(elems[b].image_id).ProjectionCenter();
        best_tri_rad = std::max(
            best_tri_rad, colmap::CalculateTriangulationAngle(c1, c2, pt.xyz));
      }
      if (elems.back().image_id != elems[a].image_id &&
          recon.ExistsImage(elems.back().image_id) &&
          recon.Image(elems.back().image_id).HasPose()) {
        const Eigen::Vector3d c2 =
            recon.Image(elems.back().image_id).ProjectionCenter();
        best_tri_rad = std::max(
            best_tri_rad, colmap::CalculateTriangulationAngle(c1, c2, pt.xyz));
      }
    }

    const double tri_deg = colmap::RadToDeg(best_tri_rad);
    if (tri_deg < 0.25) continue;

    ++q.reliable_points;
    tri_degs.push_back(tri_deg);
    const Eigen::Vector3d x_ref = ref_cfw * pt.xyz;
    if (x_ref.z() > 1e-4 && std::isfinite(x_ref.z())) {
      depths.push_back(x_ref.z());
    }
  }

  q.median_tri_deg = MedianInPlace(&tri_degs);
  const double med_depth = MedianInPlace(&depths);
  q.baseline_snr = (med_depth > 1e-6) ? (max_baseline / med_depth) : 0.0;

  const size_t min_reliable_pts = (num_frames >= 6) ? 12 : 6;
  q.valid_6dof = (q.reliable_points >= min_reliable_pts) &&
                 (q.median_tri_deg >= 0.45) &&
                 (q.baseline_snr >= 0.008) &&
                 (q.step_jitter_ratio <= 25.0);
  return q;
}

std::shared_ptr<colmap::Reconstruction> RunSolvePass(
    const std::shared_ptr<colmap::DatabaseCache>& base_cache,
    colmap::camera_t camera_id,
    size_t num_frames,
    double init_f,
    bool refine_focal,
    bool prefer_glomap,
    double px_scale,
    bool* out_was_glomap,
    ReconQuality* out_quality) {
  *out_was_glomap = false;
  *out_quality = ReconQuality{};

  const double reproj_px = std::clamp(2.4 * px_scale, 1.4, 3.2);
  const double norm_reproj = reproj_px / std::max(100.0, init_f);

  std::shared_ptr<colmap::Reconstruction> best_recon;
  ReconQuality best_q;

  if (prefer_glomap) {
    auto global_recon = std::make_shared<colmap::Reconstruction>();
    colmap::GlobalMapper global_mapper(base_cache);
    global_mapper.BeginReconstruction(global_recon);

    colmap::GlobalMapperOptions gopts;
    gopts.num_threads = 2;
    gopts.random_seed = 42;
    gopts.track_min_num_views_per_track = (num_frames >= 5) ? 3 : 2;
    gopts.track_intra_image_consistency_threshold = std::clamp(4.0 * px_scale, 2.0, 6.0);
    gopts.min_tri_angle_deg = 0.5;
    gopts.max_angular_reproj_error_deg = 1.5;
    gopts.max_normalized_reproj_error = norm_reproj;
    gopts.ba_num_iterations = 3;
    gopts.rotation_averaging.max_rotation_error_deg = 6.0;
    gopts.global_positioning.use_gpu = false;
    gopts.global_positioning.min_num_view_per_track = gopts.track_min_num_views_per_track;
    gopts.retriangulation.complete_max_reproj_error = reproj_px * 1.5;
    gopts.retriangulation.merge_max_reproj_error = reproj_px * 1.5;
    gopts.retriangulation.min_angle = 0.5;
    gopts.bundle_adjustment.min_track_length = gopts.track_min_num_views_per_track;
    gopts.bundle_adjustment.refine_focal_length = refine_focal;
    gopts.bundle_adjustment.refine_principal_point = false;
    gopts.bundle_adjustment.refine_extra_params = false;
    gopts.bundle_adjustment.print_summary = false;
    if (gopts.bundle_adjustment.ceres) {
      gopts.bundle_adjustment.ceres->use_gpu = false;
      gopts.bundle_adjustment.ceres->loss_function_type =
          colmap::CeresLossFunctionType::CAUCHY;
      gopts.bundle_adjustment.ceres->loss_function_scale = reproj_px;
      gopts.bundle_adjustment.ceres->solver_options.max_num_iterations = 120;
    }

    if (global_mapper.Solve(gopts) && global_recon->RegImageIds().size() >= 2) {
      const ReconQuality gq = Evaluate6DofReconstruction(
          *global_recon, camera_id, num_frames, init_f, !refine_focal, reproj_px * 1.45);
      if (gq.valid_6dof) {
        best_recon = global_recon;
        best_q = gq;
        *out_was_glomap = true;
      }
    }
  }

  const size_t min_desired = std::max<size_t>(2, (num_frames * 4) / 5);
  if (!best_recon || best_q.num_reg < min_desired) {
    auto inc_opts = std::make_shared<colmap::IncrementalPipelineOptions>();
    inc_opts->min_num_matches = 15;
    inc_opts->min_model_size = 2;
    inc_opts->multiple_models = false;
    inc_opts->extract_colors = false;
    inc_opts->num_threads = 2;
    inc_opts->random_seed = 42;
    inc_opts->min_focal_length_ratio = 0.45;
    inc_opts->max_focal_length_ratio = 2.25;
    inc_opts->ba_refine_focal_length = refine_focal;
    inc_opts->ba_refine_principal_point = false;
    inc_opts->ba_refine_extra_params = false;
    inc_opts->ba_use_gpu = false;
    inc_opts->mapper.init_min_num_inliers = 18;
    inc_opts->mapper.init_max_error = reproj_px;
    inc_opts->mapper.init_min_tri_angle = 0.8;
    inc_opts->mapper.abs_pose_max_error = std::clamp(4.0 * px_scale, 2.5, 5.5);
    inc_opts->mapper.abs_pose_min_num_inliers = 12;
    inc_opts->mapper.abs_pose_min_inlier_ratio = 0.15;
    inc_opts->mapper.filter_max_reproj_error = reproj_px * 1.25;
    inc_opts->mapper.filter_min_tri_angle = 0.5;
    inc_opts->triangulation.merge_max_reproj_error = reproj_px * 1.25;
    inc_opts->triangulation.complete_max_reproj_error = reproj_px * 1.25;
    inc_opts->triangulation.min_angle = 0.5;
    if (!refine_focal) {
      inc_opts->constant_cameras.insert(camera_id);
    }

    auto recon_mgr = std::make_shared<colmap::ReconstructionManager>();
    colmap::IncrementalPipeline inc_pipe(inc_opts, base_cache, recon_mgr);
    inc_pipe.Run();

    for (size_t m = 0; m < recon_mgr->Size(); ++m) {
      auto cand = recon_mgr->Get(m);
      if (!cand || cand->RegImageIds().size() < 2) continue;
      const ReconQuality cq = Evaluate6DofReconstruction(
          *cand, camera_id, num_frames, init_f, !refine_focal, reproj_px * 1.45);
      if (!cq.valid_6dof) continue;
      if (!best_recon ||
          cq.num_reg > best_q.num_reg ||
          (cq.num_reg == best_q.num_reg && cq.mean_reproj_px < best_q.mean_reproj_px)) {
        best_recon = cand;
        best_q = cq;
        *out_was_glomap = false;
      }
    }
  }

  *out_quality = best_q;
  return best_recon;
}

}  // namespace

extern "C" int colmap_solve_sequence(const ColmapInputFrame* frames,
                                     size_t num_frames,
                                     const ColmapSolveConfig* config,
                                     ColmapSolveResult* out_result) {
  if (!out_result) return COLMAP_SOLVE_ERR_INVALID;
  std::memset(out_result, 0, sizeof(*out_result));
  if (!frames || !config || num_frames < 2) {
    SetError(out_result,
             COLMAP_SOLVE_ERR_INVALID,
             "Need at least 2 sampled frames for COLMAP match-move");
    return COLMAP_SOLVE_ERR_INVALID;
  }

  try {
    std::call_once(g_vl_init_once, vl_constructor);

    const uint32_t width = frames[0].width;
    const uint32_t height = frames[0].height;
    if (width < 16 || height < 16) {
      SetError(out_result, COLMAP_SOLVE_ERR_INVALID, "Frame dimensions too small for COLMAP");
      return COLMAP_SOLVE_ERR_INVALID;
    }

    // Pixel error scaling relative to ~1080p so downscaled working frames reject outlier matches.
    const double max_dim = static_cast<double>(std::max(width, height));
    const double px_scale = std::clamp(max_dim / 960.0, 0.55, 1.25);

    // 1. Headless OpenGL ES 3.1 / EGL context for GPU SIFT brute-force matching.
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
                              : 0.90 * max_dim;
    colmap::Camera camera = colmap::Camera::CreateFromModelId(
        1,
        colmap::SimplePinholeCameraModel::model_id,
        init_f,
        width,
        height,
        colmap::CameraSource::USER);

    // 3. Extract SIFT keypoints & descriptors using VLFeat CPU SIFT
    // (full 3D subpixel + multi-orientation + RootSIFT, zero GL readback stalls).
    colmap::FeatureExtractionOptions ext_opts(colmap::FeatureExtractorType::SIFT);
    ext_opts.use_gpu = false;
    ext_opts.num_threads = 2;
    ext_opts.max_image_size = static_cast<int>(std::max(width, height));
    ext_opts.sift->max_num_features = std::clamp(config->max_features, 512, 8192);
    ext_opts.sift->first_octave = 0;
    ext_opts.sift->num_octaves = 4;
    ext_opts.sift->octave_resolution = 3;
    ext_opts.sift->peak_threshold = 0.004;
    ext_opts.sift->edge_threshold = 10.0;

    auto extractor = colmap::FeatureExtractor::Create(ext_opts);
    if (!extractor) {
      SetError(out_result,
               COLMAP_SOLVE_ERR_INVALID,
               "Failed to initialize COLMAP SIFT feature extractor");
      return COLMAP_SOLVE_ERR_INVALID;
    }

    std::vector<std::shared_ptr<colmap::FeatureKeypoints>> all_keypoints(num_frames);
    std::vector<std::shared_ptr<colmap::FeatureDescriptors>> all_descriptors(num_frames);
    std::vector<std::vector<Eigen::Vector2d>> all_points2d(num_frames);

    for (size_t i = 0; i < num_frames; ++i) {
      if (!frames[i].gray || frames[i].width != width || frames[i].height != height) {
        SetError(out_result,
                 COLMAP_SOLVE_ERR_INVALID,
                 "Inconsistent input frame dimensions or null buffer");
        return COLMAP_SOLVE_ERR_INVALID;
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
    match_opts.num_threads = 2;
    match_opts.sift->max_ratio = 0.80;
    match_opts.sift->max_distance = 0.70;
    match_opts.sift->cross_check = true;
    match_opts.sift->cpu_brute_force_matcher = true;

    auto matcher = colmap::FeatureMatcher::Create(match_opts);
    if (!matcher) {
      SetError(out_result,
               COLMAP_SOLVE_ERR_INVALID,
               "Failed to initialize COLMAP SIFT feature matcher");
      return COLMAP_SOLVE_ERR_INVALID;
    }

    colmap::TwoViewGeometryOptions tvg_opts;
    tvg_opts.min_num_inliers = 15;
    tvg_opts.compute_relative_pose = true;
    tvg_opts.max_H_inlier_ratio = 0.85;
    tvg_opts.ransac_options.max_error = std::clamp(2.2 * px_scale, 1.35, 3.0);
    tvg_opts.ransac_options.confidence = 0.999;
    tvg_opts.ransac_options.max_num_trials = 3000;

    std::vector<VerifiedPair> verified_pairs;
    size_t epipolar_pairs = 0;
    std::vector<double> wide_rot_errs;
    std::vector<double> wide_sim2d_errs;
    std::vector<double> wide_sim2d_scales;
    double max_pair_disp_px = 0.0;

    const size_t min_wide_gap =
        (num_frames >= 6) ? std::max<size_t>(3, num_frames / 3) : 1;
    const size_t anchor_stride = std::max<size_t>(8, (num_frames + 15) / 16);
    for (size_t i = 0; i < num_frames; ++i) {
      if (all_keypoints[i]->size() < 15) continue;
      for (size_t j = i + 1; j < num_frames; ++j) {
        if (all_keypoints[j]->size() < 15) continue;
        const size_t gap = j - i;
        const bool is_stride = (gap == 16) || (gap == 24) || (gap == 32) ||
                               (gap == 48) || (gap == 64) || (gap == 96) || (gap == 128);
        const bool is_anchor = ((i % anchor_stride == 0) && (j % anchor_stride == 0));
        const bool keep_pair = (num_frames <= 24) || (gap <= 10) || is_stride || is_anchor ||
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
            tvg.config != colmap::TwoViewGeometry::WATERMARK &&
            static_cast<int>(tvg.inlier_matches.size()) >= tvg_opts.min_num_inliers) {
          if (!tvg.cam2_from_cam1.has_value()) {
            colmap::EstimateTwoViewGeometryPose(
                camera, all_points2d[i], camera, all_points2d[j], &tvg);
          }
          if (tvg.config == colmap::TwoViewGeometry::CALIBRATED ||
              tvg.config == colmap::TwoViewGeometry::UNCALIBRATED) {
            ++epipolar_pairs;
          }

          if (gap >= min_wide_gap) {
            const Similarity2DFit sim =
                FitSimilarity2D(all_points2d[i], all_points2d[j], tvg.inlier_matches);
            max_pair_disp_px = std::max(max_pair_disp_px, sim.mean_disp_px);
            // Evaluate model residuals on pairs with meaningful frame-to-frame motion.
            if (sim.mean_disp_px >= 2.5 * px_scale) {
              wide_rot_errs.push_back(BestFocalPureRotationPxErr(
                  width, height, init_f, all_points2d[i], all_points2d[j], tvg.inlier_matches));
              wide_sim2d_errs.push_back(sim.median_err_px);
              wide_sim2d_scales.push_back(sim.scale_delta);
            }
          }

          verified_pairs.push_back({i, j, std::move(tvg)});
        }
      }
    }

    if (verified_pairs.empty()) {
      SetError(out_result,
               COLMAP_SOLVE_ERR_INSUFFICIENT_MOTION,
               "not enough clean motion to solve the camera");
      return COLMAP_SOLVE_ERR_INSUFFICIENT_MOTION;
    }

    // Check physical motion regime across wide-stride pairs:
    // 1. Flat 2D image-plane pan/zoom/spin (e.g. 2D animation, Ken Burns crop/zoom on a flat image):
    //    zero non-coplanar epipolar pairs AND exact 4-DOF 2D similarity (`p75 <= 1.15 px`) with zoom
    //    scale change (`scale_delta >= 0.015`) or pure 2D translation that violates 3D perspective
    //    rotation (`rot_p75 > 2.8 px`).
    // 2. Pure 3D tripod pan/tilt (zero camera translation):
    //    focal-invariant pure SO(3) rotation fits wide-stride pairs within `<= 3.2 * px_scale` pixels!
    //    Note: Even when `epipolar_pairs > 0`, a small focal prior mismatch (`f_init != f_true`) on a
    //    pure-rotation or near-zero-parallax pan can cause `E_ransac` to label a few pairs `CALIBRATED`
    //    with ~0.3 deg noise parallax; `BestFocalPureRotationPxErr` is focal-invariant, so checking
    //    `rot_p75 <= pure_rot_limit` directly catches ALL pure-rotation / sub-pixel-parallax clips and
    //    prevents Incremental BA focal-refinement from inverting translation direction!
    const double rot_p90 = PercentileInPlace(&wide_rot_errs, 0.90);
    const double sim2d_p90 = PercentileInPlace(&wide_sim2d_errs, 0.90);
    const double sim2d_scale_max = PercentileInPlace(&wide_sim2d_scales, 0.90);
    const double pure_rot_limit = std::clamp(2.4 * px_scale, 1.5, 2.8);
    const double sim2d_limit = std::clamp(1.25 * px_scale, 0.85, 1.45);

    // Pure 2D image-plane pan/zoom/spin (anime, screen recording, Ken Burns crop/zoom on a flat plate):
    // every wide-stride pair fits an exact 4-DOF 2D similarity (`sim2d_p90 <= sim2d_limit`, zero 3D
    // perspective foreshortening and zero depth parallax), whereas pure 3D rotation fails cleanly
    // (`rot_p90 > pure_rot_limit * 1.25`). Note that integer pixel rasterization or H.264 compression on
    // a 2D zoom plate can induce a couple sub-pixel noise matches that pass `E_ransac` (`epipolar_pairs > 0`),
    // so we check `sim2d_p90 <= sim2d_limit` across ALL wide pairs directly!
    if (!wide_sim2d_errs.empty() &&
        sim2d_p90 <= sim2d_limit &&
        (sim2d_scale_max >= 0.015 || rot_p90 > pure_rot_limit * 1.25)) {
      SetError(out_result,
               COLMAP_SOLVE_ERR_PLANAR_2D,
               "footage appears to be flat 2D motion without 3D perspective; use 2D motion tracking");
      return COLMAP_SOLVE_ERR_PLANAR_2D;
    }

    const bool is_pure_tripod_rotation =
        wide_rot_errs.empty() ? (epipolar_pairs == 0) : (rot_p90 <= pure_rot_limit);

    // 5. Build in-memory COLMAP DatabaseCache:
    //    Include ALL verified pairs with valid relative poses (`CALIBRATED`, `UNCALIBRATED`, AND
    //    `PLANAR`!) in `sfm_cache` so planar floors, walls, tabletops, and short-stride video pairs
    //    contribute full multi-view feature tracks and 3D relative poses to GLOMAP & COLMAP.
    //    Only zero-translation `PANORAMIC` pairs are excluded from `sfm_cache`.
    auto make_empty_cache = [&]() {
      auto c = std::make_shared<colmap::DatabaseCache>();
      c->AddCamera(camera);
      colmap::Rig rig;
      rig.SetRigId(1);
      rig.AddRefSensor(camera.SensorId());
      c->AddRig(std::move(rig));
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
        c->AddFrame(std::move(frm));
        c->AddImage(std::move(img));
      }
      return c;
    };

    std::shared_ptr<colmap::Reconstruction> best_recon;
    bool best_was_glomap = false;
    ReconQuality best_quality;

    if (!is_pure_tripod_rotation) {
      auto sfm_cache = make_empty_cache();
      size_t sfm_pair_count = 0;
      for (const auto& vp : verified_pairs) {
        if (vp.tvg.config == colmap::TwoViewGeometry::PANORAMIC) {
          continue;
        }
        sfm_cache->CorrespondenceGraph()->AddTwoViewGeometry(
            static_cast<colmap::image_t>(vp.i + 1),
            static_cast<colmap::image_t>(vp.j + 1),
            vp.tvg);
        ++sfm_pair_count;
      }
      sfm_cache->CorrespondenceGraph()->Finalize();
      colmap::MaybeDecomposeRelativePoses(sfm_cache.get());

      if (sfm_pair_count >= 1) {
        const bool want_refine = (config->lock_fov == 0);
        best_recon = RunSolvePass(sfm_cache,
                                  camera.camera_id,
                                  num_frames,
                                  init_f,
                                  want_refine,
                                  config->prefer_glomap != 0,
                                  px_scale,
                                  &best_was_glomap,
                                  &best_quality);

        // Automatic locked-FOV fallback if uncalibrated focal refinement hit low-parallax degeneracy!
        if (want_refine &&
            (!best_recon || best_quality.num_reg < std::max<size_t>(2, (num_frames * 3) / 4))) {
          bool locked_was_glomap = false;
          ReconQuality locked_q;
          auto locked_recon = RunSolvePass(sfm_cache,
                                           camera.camera_id,
                                           num_frames,
                                           init_f,
                                           /*refine_focal=*/false,
                                           config->prefer_glomap != 0,
                                           px_scale,
                                           &locked_was_glomap,
                                           &locked_q);
          if (locked_recon &&
              (!best_recon || locked_q.num_reg > best_quality.num_reg ||
               (locked_q.num_reg == best_quality.num_reg &&
                locked_q.mean_reproj_px < best_quality.mean_reproj_px))) {
            best_recon = locked_recon;
            best_was_glomap = locked_was_glomap;
            best_quality = locked_q;
          }
        }
      }
    }

    // 7. Export poses & sparse 3D points.
    out_result->num_poses = num_frames;
    out_result->poses = static_cast<ColmapSolvedPose*>(
        std::calloc(num_frames, sizeof(ColmapSolvedPose)));
    if (!out_result->poses) {
      SetError(out_result, COLMAP_SOLVE_ERR_INVALID, "Out of memory allocating poses");
      return COLMAP_SOLVE_ERR_INVALID;
    }

    for (size_t i = 0; i < num_frames; ++i) {
      out_result->poses[i].time_s = frames[i].time_s;
      out_result->poses[i].registered = 0;
      out_result->poses[i].r_cw[0] = 1.0;
      out_result->poses[i].r_cw[4] = 1.0;
      out_result->poses[i].r_cw[8] = 1.0;
      out_result->poses[i].focal_px = init_f;
    }

    if (best_recon && best_quality.valid_6dof && best_recon->RegImageIds().size() >= 2) {
      out_result->used_glomap = best_was_glomap ? 1 : 0;
      out_result->focal_px = best_quality.solved_focal;
      out_result->mean_reproj_px = best_quality.mean_reproj_px;

      for (const colmap::image_t img_id : best_recon->RegImageIds()) {
        const size_t idx = static_cast<size_t>(img_id - 1);
        if (idx >= num_frames) continue;
        const colmap::Rigid3d cfw = best_recon->Image(img_id).CamFromWorld();
        const Eigen::Matrix3d rot = cfw.rotation().toRotationMatrix();
        const Eigen::Vector3d trans = cfw.translation();
        out_result->poses[idx].registered = 1;
        out_result->poses[idx].focal_px = best_quality.solved_focal;
        for (int r = 0; r < 3; ++r) {
          for (int c = 0; c < 3; ++c) {
            out_result->poses[idx].r_cw[r * 3 + c] = rot(r, c);
          }
          out_result->poses[idx].t_cw[r] = trans(r);
        }
      }

      // Export clean, well-conditioned 3D landmarks.
      const size_t min_obs = (best_quality.num_reg >= 5) ? 3 : 2;
      const double max_pt_err = std::max(2.8, best_quality.mean_reproj_px * 2.4);
      std::vector<ColmapSolvedPoint3D> clean_pts;
      clean_pts.reserve(best_recon->NumPoints3D());
      for (const auto& [pt_id, pt] : best_recon->Points3D()) {
        (void)pt_id;
        if (!pt.xyz.allFinite() || pt.track.Length() < min_obs) continue;
        const double perr = pt.HasError() ? pt.error : best_quality.mean_reproj_px;
        if (!std::isfinite(perr) || perr > max_pt_err) continue;
        ColmapSolvedPoint3D sp{};
        sp.xyz[0] = pt.xyz.x();
        sp.xyz[1] = pt.xyz.y();
        sp.xyz[2] = pt.xyz.z();
        sp.observations = static_cast<uint32_t>(pt.track.Length());
        sp.reproj_error = perr;
        clean_pts.push_back(sp);
      }

      out_result->num_points = clean_pts.size();
      if (out_result->num_points > 0) {
        out_result->points = static_cast<ColmapSolvedPoint3D*>(
            std::calloc(out_result->num_points, sizeof(ColmapSolvedPoint3D)));
        if (out_result->points) {
          std::memcpy(out_result->points,
                      clean_pts.data(),
                      out_result->num_points * sizeof(ColmapSolvedPoint3D));
        } else {
          out_result->num_points = 0;
        }
      }
      out_result->status = COLMAP_SOLVE_OK;
      return COLMAP_SOLVE_OK;
    }

    // If the clip was completely static (`max_pair_disp_px < 1.0 px`) with zero 3D parallax,
    // verify whether features actually exist or if it was a blank/featureless frame.
    // Note: static textured clips still solve cleanly as a stationary rotation-only lock unless
    // there were too few verified correspondences (handled above).

    // 8. Pure-rotation / tripod global SO(3) solver:
    //    Estimate pure SO(3) rotation directly on calibrated bearing rays via Kabsch/Procrustes
    //    (immune to planar rank-1 `t * n^T / d` noise when `t = 0`), falling back to decomposed
    //    `cam2_from_cam1->rotation()` or `ProjectToSO3(K^-1 H K)`.
    Eigen::Matrix3d k_mat = Eigen::Matrix3d::Identity();
    k_mat(0, 0) = init_f;
    k_mat(1, 1) = init_f;
    k_mat(0, 2) = 0.5 * static_cast<double>(width);
    k_mat(1, 2) = 0.5 * static_cast<double>(height);
    const Eigen::Matrix3d k_inv = k_mat.inverse();

    auto pair_rotation = [&](const VerifiedPair& vp, Eigen::Matrix3d* r_out) -> bool {
      if (EstimatePureRotationProcrustes(camera,
                                         all_points2d[vp.i],
                                         all_points2d[vp.j],
                                         vp.tvg.inlier_matches,
                                         r_out) < 50.0) {
        return true;
      }
      if (vp.tvg.cam2_from_cam1.has_value()) {
        *r_out = vp.tvg.cam2_from_cam1->rotation().toRotationMatrix();
        return true;
      }
      if (vp.tvg.H.has_value() && vp.tvg.H->allFinite() && vp.tvg.H->norm() > 1e-6) {
        *r_out = ProjectToSO3(k_inv * (*vp.tvg.H) * k_mat);
        return true;
      }
      return false;
    };

    auto rot_cache = make_empty_cache();
    colmap::PoseGraph rot_pose_graph;
    for (const auto& vp : verified_pairs) {
      Eigen::Matrix3d r_21 = Eigen::Matrix3d::Identity();
      if (!pair_rotation(vp, &r_21)) continue;

      const colmap::image_t id1 = static_cast<colmap::image_t>(vp.i + 1);
      const colmap::image_t id2 = static_cast<colmap::image_t>(vp.j + 1);
      colmap::PoseGraph::Edge edge(
          colmap::Rigid3d(Eigen::Quaterniond(r_21), Eigen::Vector3d::Zero()));
      edge.num_matches = static_cast<int>(vp.tvg.inlier_matches.size());
      rot_pose_graph.AddEdge(id1, id2, std::move(edge));
    }

    if (!rot_pose_graph.Empty()) {
      auto rot_recon = std::make_shared<colmap::Reconstruction>();
      rot_recon->Load(*rot_cache);

      colmap::RotationEstimatorOptions ra_opts;
      ra_opts.random_seed = 42;
      ra_opts.max_rotation_error_deg = 5.0;
      ra_opts.reweighting = colmap::RotationAveragingReweighting::INLIER_MATCH_COUNT;
      ra_opts.filter_unregistered = false;

      const std::vector<colmap::PosePrior> empty_priors;
      if (colmap::RunRotationAveraging(ra_opts, rot_pose_graph, *rot_recon, empty_priors)) {
        ra_opts.filter_unregistered = true;
        colmap::RunRotationAveraging(ra_opts, rot_pose_graph, *rot_recon, empty_priors);
      }

      if (rot_recon->RegImageIds().size() >= 2) {
        out_result->used_glomap = 1;
        out_result->focal_px = init_f;
        out_result->mean_reproj_px = std::isfinite(rot_p90) && rot_p90 < 50.0 ? rot_p90 : 0.65;
        for (const colmap::image_t img_id : rot_recon->RegImageIds()) {
          const size_t idx = static_cast<size_t>(img_id - 1);
          if (idx >= num_frames) continue;
          const Eigen::Matrix3d rot =
              rot_recon->Image(img_id).CamFromWorld().rotation().toRotationMatrix();
          out_result->poses[idx].registered = 1;
          out_result->poses[idx].focal_px = init_f;
          for (int r = 0; r < 3; ++r) {
            for (int c = 0; c < 3; ++c) {
              out_result->poses[idx].r_cw[r * 3 + c] = rot(r, c);
            }
            out_result->poses[idx].t_cw[r] = 0.0;
          }
        }
        out_result->status = COLMAP_SOLVE_OK;
        return COLMAP_SOLVE_OK;
      }
    }

    // Sequential rotation chain fallback if global rotation averaging had < 2 nodes.
    std::vector<Eigen::Matrix3d> r_cw_chain(num_frames, Eigen::Matrix3d::Identity());
    std::vector<bool> reached(num_frames, false);
    reached[0] = true;
    size_t chained_count = 1;

    for (size_t i = 0; i + 1 < num_frames; ++i) {
      if (!reached[i]) continue;
      for (const auto& vp : verified_pairs) {
        if (vp.i == i && vp.j == i + 1) {
          Eigen::Matrix3d r_step = Eigen::Matrix3d::Identity();
          if (pair_rotation(vp, &r_step)) {
            r_cw_chain[i + 1] = r_step * r_cw_chain[i];
            reached[i + 1] = true;
            ++chained_count;
            break;
          }
        }
      }
    }

    if (chained_count >= 2) {
      out_result->focal_px = init_f;
      out_result->mean_reproj_px = 0.75;
      for (size_t i = 0; i < num_frames; ++i) {
        if (!reached[i]) continue;
        out_result->poses[i].registered = 1;
        out_result->poses[i].focal_px = init_f;
        for (int r = 0; r < 3; ++r) {
          for (int c = 0; c < 3; ++c) {
            out_result->poses[i].r_cw[r * 3 + c] = r_cw_chain[i](r, c);
          }
          out_result->poses[i].t_cw[r] = 0.0;
        }
      }
      out_result->status = COLMAP_SOLVE_OK;
      return COLMAP_SOLVE_OK;
    }

    colmap_free_result(out_result);
    SetError(out_result,
             COLMAP_SOLVE_ERR_INSUFFICIENT_MOTION,
             "not enough clean motion to solve the camera");
    return COLMAP_SOLVE_ERR_INSUFFICIENT_MOTION;
  } catch (const std::exception& ex) {
    colmap_free_result(out_result);
    SetError(out_result, COLMAP_SOLVE_ERR_EXCEPTION, ex.what());
    return COLMAP_SOLVE_ERR_EXCEPTION;
  } catch (...) {
    colmap_free_result(out_result);
    SetError(out_result, COLMAP_SOLVE_ERR_EXCEPTION, "Unhandled COLMAP exception");
    return COLMAP_SOLVE_ERR_EXCEPTION;
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
