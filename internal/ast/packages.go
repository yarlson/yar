package ast

type PackageImport struct {
	Name string
	Path string
	Decl ImportDecl
}

type Package struct {
	Path      string
	Name      string
	Files     []*Program
	Imports   []PackageImport
	Structs   []*StructDecl
	Functions []*FunctionDecl
}

type PackageGraph struct {
	EntryPath string
	Entry     *Package
	Packages  map[string]*Package
}
