// SPDX-License-Identifier: BSD-3-Clause
#pragma once

#include <Eigen/SparseCholesky>
#include <Eigen/SparseCore>

namespace colmap {

class SparseCholeskyWithFallbackSolver {
 public:
  bool Compute(const Eigen::SparseMatrix<double>& A);
  void AnalyzePattern(const Eigen::SparseMatrix<double>& A);
  bool Factorize(const Eigen::SparseMatrix<double>& A);
  bool Solve(const Eigen::VectorXd& b, Eigen::VectorXd* x) const;

 private:
  Eigen::SimplicialLLT<Eigen::SparseMatrix<double>> llt_;
  Eigen::SimplicialLDLT<Eigen::SparseMatrix<double>> ldlt_;
  bool use_ldlt_ = false;
};

}  // namespace colmap
