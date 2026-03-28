package main

import "fs"
import "path"

fn kind_code(kind fs.EntryKind) i32 {
    match kind {
    case fs.EntryKind.File {
        return 1
    }
    case fs.EntryKind.Directory {
        return 2
    }
    case fs.EntryKind.Other {
        return 3
    }
    }
}

fn fails(path str) i32 {
    fs.read_file(path) or |err| {
        return 1
    }
    return 0
}

fn main() !i32 {
    root := fs.temp_dir("yar-fs-path-")?
    nested := path.join([]str{root, "nested", "deeper"})
    fs.mkdir_all(nested)?

    file_path := path.join([]str{nested, "sample.yar"})
    cleaned := path.clean(path.join([]str{root, "nested", ".", "deeper", "..", "deeper", "sample.yar"}))
    if cleaned != file_path {
        return 1
    }
    if path.dir(file_path) != nested {
        return 1
    }
    if path.base(file_path) != "sample.yar" {
        return 1
    }
    if path.ext(file_path) != ".yar" {
        return 1
    }

    fs.write_file(file_path, "hello from fs")?
    data := fs.read_file(file_path)?
    if data != "hello from fs" {
        return 1
    }

    file_kind := fs.stat(file_path)?
    if kind_code(file_kind) != 1 {
        return 1
    }

    dir_kind := fs.stat(nested)?
    if kind_code(dir_kind) != 2 {
        return 1
    }

    entries := fs.read_dir(nested)?
    found_file := false
    i := 0
    for i < len(entries) {
        entry := entries[i]
        if entry.name == "sample.yar" && !entry.is_dir {
            found_file = true
        }
        i = i + 1
    }
    if !found_file {
        return 1
    }

    missing_path := path.join([]str{root, "missing.txt"})
    if fails(missing_path) != 1 {
        return 1
    }

    fs.remove_all(root)?
    if fails(root) != 1 {
        return 1
    }
    print("fs_path ok\n")
    return 0
}
