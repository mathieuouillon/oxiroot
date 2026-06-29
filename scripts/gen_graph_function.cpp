// Dev-only: regenerate fixtures/graph_function.root.
//
// Writes a TGraph named "gfit" carrying a TF1 ("line", "[0]+[1]*x") in its
// fFunctions list, the way ROOT stores a fitted function on a graph. oxiroot
// reads this fixture (the TF1/TFormula it parses must match ROOT), and ROOT C++
// and uproot must read oxiroot's round-trip of it.
//
//   c++ $(root-config --cflags) scripts/gen_graph_function.cpp \
//       $(root-config --libs) -o /tmp/gen_graph_function
//   /tmp/gen_graph_function           # writes fixtures/graph_function.root
#include <TFile.h>
#include <TGraph.h>
#include <TF1.h>
#include <TList.h>

int main() {
  TFile f("fixtures/graph_function.root", "RECREATE");
  f.SetCompressionLevel(0);

  double x[5] = {0, 1, 2, 3, 4};
  double y[5] = {1, 3, 5, 7, 9};
  TGraph g(5, x, y);
  g.SetName("gfit");
  g.SetTitle("fitted");

  // A formula TF1 with set parameters, added to the graph's function list the
  // way TGraph::Fit would (without running a fit, which the local cling JIT
  // mishandles).
  TF1* fn = new TF1("line", "[0]+[1]*x", 0, 4);
  fn->SetParameters(1, 2);
  g.GetListOfFunctions()->Add(fn);

  g.Write();
  f.Close();
  return 0;
}
