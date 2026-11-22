// SPDX-License-Identifier: BSD-3-Clause
#include "colmap/math/spanning_tree.h"

#include <algorithm>
#include <numeric>
#include <queue>
#include <vector>

namespace colmap {
namespace {

void BuildParentsFromAdjacencyList(
    const std::vector<std::vector<int>>& adjacency_list,
    int root,
    std::vector<int>& parents) {
  const int num_nodes = static_cast<int>(adjacency_list.size());
  parents.assign(num_nodes, -1);
  if (root < 0 || root >= num_nodes) return;
  parents[root] = root;

  std::vector<char> visited(num_nodes, false);
  visited[root] = true;

  std::queue<int> queue;
  queue.push(root);
  while (!queue.empty()) {
    const int current = queue.front();
    queue.pop();
    for (const int neighbor : adjacency_list[current]) {
      if (!visited[neighbor]) {
        visited[neighbor] = true;
        parents[neighbor] = current;
        queue.push(neighbor);
      }
    }
  }
}

SpanningTree ComputeSpanningTreeInternal(
    int num_nodes,
    const std::vector<std::pair<int, int>>& edges,
    const std::vector<float>& weights,
    int root,
    bool maximize) {
  SpanningTree tree;
  if (num_nodes <= 0) {
    return tree;
  }

  std::vector<size_t> order(edges.size());
  std::iota(order.begin(), order.end(), 0);
  std::sort(order.begin(), order.end(), [&](size_t a, size_t b) {
    return maximize ? (weights[a] > weights[b]) : (weights[a] < weights[b]);
  });

  std::vector<int> uf_parent(num_nodes);
  std::vector<int> uf_rank(num_nodes, 0);
  std::iota(uf_parent.begin(), uf_parent.end(), 0);

  auto find_root = [&](int x) {
    while (uf_parent[x] != x) {
      uf_parent[x] = uf_parent[uf_parent[x]];
      x = uf_parent[x];
    }
    return x;
  };

  std::vector<std::vector<int>> adjacency_list(num_nodes);
  for (size_t idx : order) {
    const int u = edges[idx].first;
    const int v = edges[idx].second;
    if (u < 0 || u >= num_nodes || v < 0 || v >= num_nodes || u == v) continue;
    const int ru = find_root(u);
    const int rv = find_root(v);
    if (ru == rv) continue;
    if (uf_rank[ru] < uf_rank[rv]) {
      uf_parent[ru] = rv;
    } else if (uf_rank[ru] > uf_rank[rv]) {
      uf_parent[rv] = ru;
    } else {
      uf_parent[rv] = ru;
      ++uf_rank[ru];
    }
    adjacency_list[u].push_back(v);
    adjacency_list[v].push_back(u);
  }

  tree.root = root;
  BuildParentsFromAdjacencyList(adjacency_list, tree.root, tree.parents);
  return tree;
}

}  // namespace

SpanningTree ComputeMaximumSpanningTree(
    int num_nodes,
    const std::vector<std::pair<int, int>>& edges,
    const std::vector<float>& weights,
    int root) {
  return ComputeSpanningTreeInternal(num_nodes, edges, weights, root, true);
}

SpanningTree ComputeMinimumSpanningTree(
    int num_nodes,
    const std::vector<std::pair<int, int>>& edges,
    const std::vector<float>& weights,
    int root) {
  return ComputeSpanningTreeInternal(num_nodes, edges, weights, root, false);
}

}  // namespace colmap
