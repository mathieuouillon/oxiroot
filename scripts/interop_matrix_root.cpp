// ROOT C++ side of the manifest-driven interop matrix. Reads manifest.json
// (written by crates/oxiroot/examples/interop_matrix.rs), opens each
// Rust-written .root file, and asserts ROOT's parse matches the manifest.
//
//   c++ $(root-config --cflags) scripts/interop_matrix_root.cpp \
//       $(root-config --libs) -lROOTNTuple -o /tmp/interop_matrix_root
//   /tmp/interop_matrix_root <dir>     # <dir> holds the .root files + manifest.json
//
// Prints "PASS <id>" / "MISMATCH <id>: <detail>" per case; exits nonzero if any
// case failed. No struct dictionaries are linked — split std::vector<Struct>
// branches are read dict-free via TTreeReaderArray on the embedded TStreamerInfo.

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

#include <TDirectory.h>
#include <TFile.h>
#include <TH1.h>
#include <TH2.h>
#include <TH3.h>
#include <TKey.h>
#include <TLeaf.h>
#include <TProfile.h>
#include <TTree.h>
#include <TTreeReader.h>
#include <TTreeReaderArray.h>

#include <ROOT/RNTupleReader.hxx>

#include "json_mini.hpp"

static int g_fail = 0;
static int g_pass = 0;
static std::string g_id;
static std::string g_err;  // first error detail for the current case

static void mark(bool ok, const std::string& detail) {
  if (!ok && g_err.empty()) g_err = detail;
}
static bool close(double a, double b) {
  return std::fabs(a - b) <= 1e-6 * std::max(1.0, std::fabs(b));
}

static std::string read_file(const std::string& path) {
  std::ifstream f(path, std::ios::binary);
  std::ostringstream ss;
  ss << f.rdbuf();
  return ss.str();
}

// ---- histogram checks --------------------------------------------------------

static void check_edges(TAxis* ax, const jsonm::Value& edges) {
  int nb = ax->GetNbins();
  if ((int)edges.size() != nb + 1) {
    mark(false, "edge count " + std::to_string(edges.size()) + " != " + std::to_string(nb + 1));
    return;
  }
  for (int k = 0; k <= nb; ++k)
    mark(close(ax->GetBinLowEdge(k + 1), edges[(size_t)k].as_num()),
         "edge " + std::to_string(k));
}

static void check_hist(TFile* f, const std::string& name, const std::string& cls,
                       int dim, const jsonm::Value& c) {
  TObject* o = f->Get(name.c_str());
  if (!o) {
    mark(false, "no object " + name);
    return;
  }
  if (cls != o->ClassName()) mark(false, std::string("class ") + o->ClassName() + " != " + cls);
  const jsonm::Value& vals = c["values"];
  const jsonm::Value& edges = c["edges"];
  bool has_err = c.has("sumw2_error");
  const jsonm::Value& errs = c["sumw2_error"];

  if (dim == 1) {
    TH1* h = dynamic_cast<TH1*>(o);
    if (!h) { mark(false, "not TH1"); return; }
    check_edges(h->GetXaxis(), edges[(size_t)0]);
    for (size_t i = 0; i < vals.size(); ++i) {
      mark(close(h->GetBinContent((int)i + 1), vals[i].as_num()), "bin " + std::to_string(i));
      if (has_err) mark(close(h->GetBinError((int)i + 1), errs[i].as_num()), "err " + std::to_string(i));
    }
  } else if (dim == 2) {
    TH2* h = dynamic_cast<TH2*>(o);
    if (!h) { mark(false, "not TH2"); return; }
    check_edges(h->GetXaxis(), edges[(size_t)0]);
    check_edges(h->GetYaxis(), edges[(size_t)1]);
    for (size_t ix = 0; ix < vals.size(); ++ix)
      for (size_t iy = 0; iy < vals[ix].size(); ++iy) {
        mark(close(h->GetBinContent((int)ix + 1, (int)iy + 1), vals[ix][iy].as_num()),
             "bin " + std::to_string(ix) + "," + std::to_string(iy));
        if (has_err)
          mark(close(h->GetBinError((int)ix + 1, (int)iy + 1), errs[ix][iy].as_num()),
               "err " + std::to_string(ix) + "," + std::to_string(iy));
      }
  } else {
    TH3* h = dynamic_cast<TH3*>(o);
    if (!h) { mark(false, "not TH3"); return; }
    check_edges(h->GetXaxis(), edges[(size_t)0]);
    check_edges(h->GetYaxis(), edges[(size_t)1]);
    check_edges(h->GetZaxis(), edges[(size_t)2]);
    for (size_t ix = 0; ix < vals.size(); ++ix)
      for (size_t iy = 0; iy < vals[ix].size(); ++iy)
        for (size_t iz = 0; iz < vals[ix][iy].size(); ++iz) {
          mark(close(h->GetBinContent((int)ix + 1, (int)iy + 1, (int)iz + 1), vals[ix][iy][iz].as_num()),
               "bin3");
          if (has_err)
            mark(close(h->GetBinError((int)ix + 1, (int)iy + 1, (int)iz + 1), errs[ix][iy][iz].as_num()),
                 "err3");
        }
  }
  if (c.has("entries")) {
    TH1* h = dynamic_cast<TH1*>(o);
    if (h) mark(close(h->GetEntries(), c["entries"].as_num()), "entries");
  }
}

static void check_profile(TFile* f, const jsonm::Value& c) {
  TProfile* p = dynamic_cast<TProfile*>(f->Get(c["name"].as_str().c_str()));
  if (!p) { mark(false, "no TProfile"); return; }
  check_edges(p->GetXaxis(), c["edges"][(size_t)0]);
  const jsonm::Value& vals = c["values"];
  for (size_t i = 0; i < vals.size(); ++i)
    mark(close(p->GetBinContent((int)i + 1), vals[i].as_num()), "mean " + std::to_string(i));
}

// ---- RNTuple checks ----------------------------------------------------------

template <typename T>
static void rn_scalar(ROOT::RNTupleReader* r, const std::string& name, const jsonm::Value& vals,
                      bool isbool) {
  auto v = r->GetView<T>(name);
  for (size_t i = 0; i < vals.size(); ++i) {
    double want = isbool ? (vals[i].as_bool() ? 1.0 : 0.0) : vals[i].as_num();
    mark(close((double)v(i), want), name + "[" + std::to_string(i) + "]");
  }
}

template <typename T>
static void rn_vector(ROOT::RNTupleReader* r, const std::string& name, const jsonm::Value& vals,
                      bool isbool) {
  auto v = r->GetView<std::vector<T>>(name);
  for (size_t i = 0; i < vals.size(); ++i) {
    const std::vector<T>& got = v(i);
    if (got.size() != vals[i].size()) {
      mark(false, name + "[" + std::to_string(i) + "] size");
      continue;
    }
    for (size_t j = 0; j < got.size(); ++j) {
      double want = isbool ? (vals[i][j].as_bool() ? 1.0 : 0.0) : vals[i][j].as_num();
      mark(close((double)got[j], want), name + "[" + std::to_string(i) + "][" + std::to_string(j) + "]");
    }
  }
}

static void check_rntuple(const std::string& path, const jsonm::Value& c) {
  auto reader = ROOT::RNTupleReader::Open(c["name"].as_str(), path);
  if ((long long)reader->GetNEntries() != c["n_entries"].as_int()) {
    mark(false, "n_entries " + std::to_string(reader->GetNEntries()));
    return;
  }
  for (const auto& fld : c["fields"].arr) {
    std::string name = fld["name"].as_str();
    std::string ty = fld["type"].as_str();
    const jsonm::Value& vals = fld["values"];
    if (ty == "bool") rn_scalar<bool>(reader.get(), name, vals, true);
    else if (ty == "int32") rn_scalar<std::int32_t>(reader.get(), name, vals, false);
    else if (ty == "int64") rn_scalar<std::int64_t>(reader.get(), name, vals, false);
    else if (ty == "uint32") rn_scalar<std::uint32_t>(reader.get(), name, vals, false);
    else if (ty == "uint64") rn_scalar<std::uint64_t>(reader.get(), name, vals, false);
    else if (ty == "float32") rn_scalar<float>(reader.get(), name, vals, false);
    else if (ty == "float64") rn_scalar<double>(reader.get(), name, vals, false);
    else if (ty == "string") {
      auto v = reader->GetView<std::string>(name);
      for (size_t i = 0; i < vals.size(); ++i)
        mark(v(i) == vals[i].as_str(), name + "[" + std::to_string(i) + "]");
    } else if (ty == "vector<bool>") rn_vector<bool>(reader.get(), name, vals, true);
    else if (ty == "vector<int32>") rn_vector<std::int32_t>(reader.get(), name, vals, false);
    else if (ty == "vector<int64>") rn_vector<std::int64_t>(reader.get(), name, vals, false);
    else if (ty == "vector<float32>") rn_vector<float>(reader.get(), name, vals, false);
    else if (ty == "vector<float64>") rn_vector<double>(reader.get(), name, vals, false);
    else mark(false, "unknown rntuple type " + ty);
  }
}

// ---- TTree checks ------------------------------------------------------------

static void check_tree_branches(TTree* t, const jsonm::Value& branches) {
  for (const auto& b : branches.arr) {
    std::string name = b["name"].as_str();
    std::string leaf = b["leaf"].as_str();
    std::string ty = b.has("type") ? b["type"].as_str() : "";
    const jsonm::Value& vals = b["values"];

    if (leaf == "scalar") {
      // Read every scalar leaf through TLeaf::GetValue() (returns a double). This
      // is type-agnostic and, crucially, the only reliable ROOT path for a
      // `Char_t` (`/B`, int8) leaf: SetBranchAddress(signed char*)+GetEntry reads
      // it as 0 (ROOT's Char_t-is-a-string ambiguity — ROOT mis-reads even its
      // OWN i8 that way), whereas GetValue() yields the correct value. Our test
      // values stay below 2^53 so the double conversion is exact.
      t->ResetBranchAddresses();
      TLeaf* lf = t->GetLeaf(name.c_str());
      if (!lf) {
        mark(false, "no leaf " + name);
      } else {
        for (size_t i = 0; i < vals.size(); ++i) {
          t->GetEntry((Long64_t)i);
          double want = (ty == "bool") ? (vals[i].as_bool() ? 1.0 : 0.0) : vals[i].as_num();
          mark(close(lf->GetValue(), want), name + "[" + std::to_string(i) + "]");
        }
      }
      t->ResetBranchAddresses();
    } else if (leaf == "fixed") {
      int dim = (int)b["dim"].as_int();
      t->ResetBranchAddresses();
      std::vector<double> buf(dim);
      t->SetBranchAddress(name.c_str(), buf.data());
      for (size_t i = 0; i < vals.size(); ++i) {
        t->GetEntry((Long64_t)i);
        for (int j = 0; j < dim; ++j)
          mark(close(buf[j], vals[i][(size_t)j].as_num()), name + " fixed");
      }
      t->ResetBranchAddresses();
    } else if (leaf == "jagged") {
      std::string count = b["count"].as_str();
      t->ResetBranchAddresses();
      Int_t n = 0;
      std::vector<double> buf(64);
      t->SetBranchAddress(count.c_str(), &n);
      t->SetBranchAddress(name.c_str(), buf.data());
      for (size_t i = 0; i < vals.size(); ++i) {
        t->GetEntry((Long64_t)i);
        if ((size_t)n != vals[i].size()) { mark(false, name + " jagged size"); continue; }
        for (int j = 0; j < n; ++j)
          mark(close(buf[j], vals[i][(size_t)j].as_num()), name + " jagged");
      }
      t->ResetBranchAddresses();
    } else if (leaf == "string") {
      t->ResetBranchAddresses();
      char buf[512] = {0};
      t->SetBranchAddress(name.c_str(), buf);
      for (size_t i = 0; i < vals.size(); ++i) {
        t->GetEntry((Long64_t)i);
        mark(std::string(buf) == vals[i].as_str(), name + " str");
      }
      t->ResetBranchAddresses();
    } else if (leaf == "stl_vector") {
      t->ResetBranchAddresses();
      std::vector<double>* vec = nullptr;
      t->SetBranchAddress(name.c_str(), &vec);
      for (size_t i = 0; i < vals.size(); ++i) {
        t->GetEntry((Long64_t)i);
        if (!vec || vec->size() != vals[i].size()) { mark(false, name + " stl size"); continue; }
        for (size_t j = 0; j < vec->size(); ++j)
          mark(close((*vec)[j], vals[i][j].as_num()), name + " stl");
      }
      t->ResetBranchAddresses();
    } else {
      mark(false, "unknown leaf " + leaf);
    }
  }
}

static void check_tree_split(TFile* f, const std::string& tname, const jsonm::Value& split) {
  std::string br = split["branch"].as_str();  // e.g. "hits"
  TTreeReader reader(tname.c_str(), f);
  // One TTreeReaderArray per member (float or int), read dict-free.
  std::vector<std::string> names;
  std::vector<char> kinds;  // 'f' or 'i'
  for (const auto& m : split["members"].arr) {
    names.push_back(br + "." + m["name"].as_str());
    kinds.push_back(m["type"].as_str().rfind("float", 0) == 0 ? 'f' : 'i');
  }
  std::vector<TTreeReaderArray<float>*> fa;
  std::vector<TTreeReaderArray<int>*> ia;
  for (size_t k = 0; k < names.size(); ++k) {
    if (kinds[k] == 'f') { fa.push_back(new TTreeReaderArray<float>(reader, names[k].c_str())); ia.push_back(nullptr); }
    else { ia.push_back(new TTreeReaderArray<int>(reader, names[k].c_str())); fa.push_back(nullptr); }
  }
  size_t entry = 0;
  while (reader.Next()) {
    for (size_t k = 0; k < names.size(); ++k) {
      const jsonm::Value& mvals = split["members"][k]["values"];
      size_t want_n = mvals[entry].size();
      size_t got_n = kinds[k] == 'f' ? fa[k]->GetSize() : ia[k]->GetSize();
      if (got_n != want_n) { mark(false, names[k] + " size"); continue; }
      for (size_t j = 0; j < got_n; ++j) {
        double got = kinds[k] == 'f' ? (double)(*fa[k])[j] : (double)(*ia[k])[j];
        mark(close(got, mvals[entry][j].as_num()), names[k]);
      }
    }
    ++entry;
  }
  for (auto* p : fa) delete p;
  for (auto* p : ia) delete p;
}

// ---- dispatch ----------------------------------------------------------------

static void run_case(const std::string& dir, const jsonm::Value& c) {
  g_id = c["id"].as_str();
  g_err.clear();
  std::string kind = c["kind"].as_str();
  std::string file = c.has("file") ? (dir + "/" + c["file"].as_str()) : "";

  if (kind == "hist") {
    TFile* f = TFile::Open(file.c_str());
    if (!f || f->IsZombie()) mark(false, "open " + file);
    else check_hist(f, c["name"].as_str(), c["class"].as_str(), (int)c["dim"].as_int(), c);
    if (f) f->Close();
  } else if (kind == "profile") {
    TFile* f = TFile::Open(file.c_str());
    if (!f || f->IsZombie()) mark(false, "open " + file);
    else check_profile(f, c);
    if (f) f->Close();
  } else if (kind == "hist_multi") {
    TFile* f = TFile::Open(file.c_str());
    if (!f || f->IsZombie()) mark(false, "open " + file);
    else
      for (const auto& o : c["objects"].arr)
        check_hist(f, o["name"].as_str(), o["class"].as_str(), (int)o["dim"].as_int(), o);
    if (f) f->Close();
  } else if (kind == "hist_dirs") {
    TFile* f = TFile::Open(file.c_str());
    if (!f || f->IsZombie()) { mark(false, "open " + file); }
    else {
      for (const auto& o : c["root_objects"].arr)
        check_hist(f, o["name"].as_str(), o["class"].as_str(), (int)o["dim"].as_int(), o);
      for (const auto& d : c["dirs"].arr) {
        TDirectory* sub = f->GetDirectory(d["dir"].as_str().c_str());
        if (!sub) { mark(false, "no dir " + d["dir"].as_str()); continue; }
        for (const auto& o : d["objects"].arr) {
          TObject* obj = sub->Get(o["name"].as_str().c_str());
          if (!obj) { mark(false, "no " + o["name"].as_str()); continue; }
          // Reuse check_hist by faking a one-object file lookup through the dir.
          int dim = (int)o["dim"].as_int();
          TH1* h = dynamic_cast<TH1*>(obj);
          if (!h) { mark(false, "dir obj not TH1"); continue; }
          const jsonm::Value& vals = o["values"];
          const jsonm::Value& edges = o["edges"];
          check_edges(h->GetXaxis(), edges[(size_t)0]);
          if (dim == 1)
            for (size_t i = 0; i < vals.size(); ++i)
              mark(close(h->GetBinContent((int)i + 1), vals[i].as_num()), "dir bin");
        }
      }
    }
    if (f) f->Close();
  } else if (kind == "rntuple" || kind == "rntuple_stream") {
    check_rntuple(file, c);
  } else if (kind == "tree") {
    TFile* f = TFile::Open(file.c_str());
    if (!f || f->IsZombie()) { mark(false, "open " + file); }
    else {
      std::string tname = c["name"].as_str();
      if (c.has("split")) {
        check_tree_split(f, tname, c["split"]);
      } else {
        TTree* t = dynamic_cast<TTree*>(f->Get(tname.c_str()));
        if (!t) mark(false, "no tree " + tname);
        else check_tree_branches(t, c["branches"]);
      }
    }
    if (f) f->Close();
  } else {
    mark(false, "unknown kind " + kind);
  }

  if (g_err.empty()) {
    std::printf("PASS %s\n", g_id.c_str());
    g_pass++;
  } else {
    std::printf("MISMATCH %s: %s\n", g_id.c_str(), g_err.c_str());
    g_fail++;
  }
}

int main(int argc, char** argv) {
  if (argc != 2) {
    std::fprintf(stderr, "usage: interop_matrix_root <dir>\n");
    return 2;
  }
  std::string dir = argv[1];
  jsonm::Value m = jsonm::parse(read_file(dir + "/manifest.json"));
  for (const auto& c : m["cases"].arr) run_case(dir, c);
  std::printf("ROOT C++ matrix: %d passed, %d failed\n", g_pass, g_fail);
  return g_fail ? 1 : 0;
}
