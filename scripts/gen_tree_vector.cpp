// Generate a TTree with std::vector<T> (TBranchElement) branches, for the Rust
// reader's TBranchElement tests. Needs ROOT 6.x; the compiled binary runs even
// where the cling JIT is broken.
//
//   c++ $(root-config --cflags) scripts/gen_tree_vector.cpp $(root-config --libs) -o /tmp/gen_tree_vector
//   /tmp/gen_tree_vector fixtures/tree_vector.root
//
// Canonical dataset (4 entries) — must match crates/oxiroot-tree/tests/read_vector.rs:
//   n  (int)            = [0, 1, 2, 3]
//   vf (vector<float>)  = [[1,2,3], [], [4], [5,6]]
//   vd (vector<double>) = [[1.5], [2.5,3.5], [], []]
//   vi (vector<int>)    = [[10,20], [30], [], [40,50,60]]

#include <cstdint>
#include <cstdio>
#include <vector>

#include <TFile.h>
#include <TTree.h>

int main(int argc, char **argv) {
    const char *path = argc > 1 ? argv[1] : "tree_vector.root";
    TFile f(path, "RECREATE");
    TTree t("T", "vector branches");

    std::int32_t n = 0;
    std::vector<float> vf;
    std::vector<double> vd;
    std::vector<int> vi;
    t.Branch("n", &n);
    t.Branch("vf", &vf);
    t.Branch("vd", &vd);
    t.Branch("vi", &vi);

    const std::vector<std::vector<float>> VF = {{1, 2, 3}, {}, {4}, {5, 6}};
    const std::vector<std::vector<double>> VD = {{1.5}, {2.5, 3.5}, {}, {}};
    const std::vector<std::vector<int>> VI = {{10, 20}, {30}, {}, {40, 50, 60}};

    for (int i = 0; i < 4; ++i) {
        n = i;
        vf = VF[i];
        vd = VD[i];
        vi = VI[i];
        t.Fill();
    }
    t.Write();
    f.Close();
    std::printf("wrote %s\n", path);
    return 0;
}
