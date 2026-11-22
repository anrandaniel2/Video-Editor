#include "lm_impl.h"

#include <cstdio>
namespace poselib {

void print_iteration(const BundleStats &stats, RobustLoss *loss_fn) {
    (void)loss_fn;
    if (stats.iterations == 0) {
        std::fprintf(stderr, "initial_cost=%g\n", stats.initial_cost);
    }
    std::fprintf(stderr,
                 "iter=%zu, cost=%g, step=%g, grad=%g, lambda=%g\n",
                 stats.iterations,
                 stats.cost,
                 stats.step_norm,
                 stats.grad_norm,
                 stats.lambda);
}

} // namespace poselib