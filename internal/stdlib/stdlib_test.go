package stdlib

import "testing"

func TestHasStrings(t *testing.T) {
	if !Has("strings") {
		t.Fatal("expected strings to be a stdlib package")
	}
}

func TestHasSort(t *testing.T) {
	if !Has("sort") {
		t.Fatal("expected sort to be a stdlib package")
	}
}

func TestHasHostPackages(t *testing.T) {
	for _, pkg := range []string{"fs", "path"} {
		if !Has(pkg) {
			t.Fatalf("expected %s to be a stdlib package", pkg)
		}
	}
}

func TestHasNonexistent(t *testing.T) {
	if Has("nonexistent") {
		t.Fatal("expected nonexistent to not be a stdlib package")
	}
}

func TestReadDirStrings(t *testing.T) {
	names, err := ReadDir("strings")
	if err != nil {
		t.Fatal(err)
	}
	if len(names) == 0 {
		t.Fatal("expected at least one .yar file in strings")
	}
	found := false
	for _, n := range names {
		if n == "strings.yar" {
			found = true
		}
	}
	if !found {
		t.Fatalf("expected strings.yar in %v", names)
	}
}

func TestReadFileStrings(t *testing.T) {
	src, err := ReadFile("strings", "strings.yar")
	if err != nil {
		t.Fatal(err)
	}
	if src == "" {
		t.Fatal("expected non-empty source")
	}
}

func TestReadFileSort(t *testing.T) {
	src, err := ReadFile("sort", "sort.yar")
	if err != nil {
		t.Fatal(err)
	}
	if src == "" {
		t.Fatal("expected non-empty source")
	}
}

func TestReadFileHostPackages(t *testing.T) {
	for _, tc := range []struct {
		pkg  string
		file string
	}{
		{pkg: "fs", file: "fs.yar"},
		{pkg: "path", file: "path.yar"},
	} {
		src, err := ReadFile(tc.pkg, tc.file)
		if err != nil {
			t.Fatalf("read %s/%s: %v", tc.pkg, tc.file, err)
		}
		if src == "" {
			t.Fatalf("expected non-empty source for %s/%s", tc.pkg, tc.file)
		}
	}
}
