// SPDX-License-Identifier: BSD-3-Clause
#include "colmap/feature/index.h"
#include "colmap/util/logging.h"

#include <algorithm>
#include <limits>
#include <numeric>
#include <vector>

namespace colmap {
namespace {

class EigenBruteForceDescriptorIndex : public FeatureDescriptorIndex {
 public:
  void Build(const FeatureDescriptorsFloat& descriptors) override {
    db_ = descriptors.data;
    norms_ = db_.rowwise().squaredNorm();
  }

  void Search(int num_neighbors,
              const FeatureDescriptorsFloat& query_descriptors,
              Eigen::RowMajorMatrixXi& indices,
              Eigen::RowMajorMatrixXf& l2_dists) const override {
    THROW_CHECK_GT(num_neighbors, 0);
    const Eigen::Index nq = query_descriptors.data.rows();
    const Eigen::Index nd = db_.rows();
    const int k = std::min<int>(num_neighbors, static_cast<int>(nd));
    indices.setConstant(nq, num_neighbors, -1);
    l2_dists.setConstant(nq, num_neighbors, std::numeric_limits<float>::max());
    if (nq == 0 || nd == 0) return;

    const Eigen::VectorXf q_norms = query_descriptors.data.rowwise().squaredNorm();
    const Eigen::MatrixXf dots = query_descriptors.data * db_.transpose();

    std::vector<int> order(static_cast<size_t>(nd));
    for (Eigen::Index qi = 0; qi < nq; ++qi) {
      if (num_neighbors == 2 && nd >= 2) {
        int b1 = -1, b2 = -1;
        float d1 = std::numeric_limits<float>::max();
        float d2 = std::numeric_limits<float>::max();
        const float qn = q_norms(qi);
        for (Eigen::Index di = 0; di < nd; ++di) {
          const float dist = std::max(0.0f, qn + norms_(di) - 2.0f * dots(qi, di));
          if (dist < d1) {
            d2 = d1; b2 = b1;
            d1 = dist; b1 = static_cast<int>(di);
          } else if (dist < d2) {
            d2 = dist; b2 = static_cast<int>(di);
          }
        }
        indices(qi, 0) = b1; l2_dists(qi, 0) = d1;
        indices(qi, 1) = b2; l2_dists(qi, 1) = d2;
      } else {
        std::iota(order.begin(), order.end(), 0);
        const float qn = q_norms(qi);
        auto dist_of = [&](int di) {
          return std::max(0.0f, qn + norms_(di) - 2.0f * dots(qi, di));
        };
        std::partial_sort(order.begin(), order.begin() + k, order.end(),
                          [&](int a, int b) { return dist_of(a) < dist_of(b); });
        for (int j = 0; j < k; ++j) {
          indices(qi, j) = order[j];
          l2_dists(qi, j) = dist_of(order[j]);
        }
      }
    }
  }

 private:
  FeatureDescriptorsFloatData db_;
  Eigen::VectorXf norms_;
};

}  // namespace

std::unique_ptr<FeatureDescriptorIndex> FeatureDescriptorIndex::Create(
    Type, int) {
  return std::make_unique<EigenBruteForceDescriptorIndex>();
}

}  // namespace colmap
