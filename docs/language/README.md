# YAR Language Design

This directory contains language identity, future planning, decision rationale,
and proposal records. It does not replace the current public reference.

## Start here

- [Current public language reference](../YAR.md)
- [Design vision](vision.md)
- [Design process and document ownership](process.md)
- [Future-only roadmap](roadmap.md)
- [Decision rationale](decisions.md)
- [Proposal template](proposal-template.md)
- [Contributor-facing current context](../context/context-map.md)
- [The Yar Code](the-yar-code.md)

Code and executable tests are the behavioral authority. `docs/YAR.md` owns
public current behavior, `docs/context/` owns current internal behavior, and
`LLM.txt` is a derived compact mirror. Proposals preserve design and evidence;
their status does not prove implementation.

## Proposal states

`Status` is design-only: `exploring`, `proposed`, `accepted`, `rejected`,
`deferred`, or `withdrawn`.

`Implementation` is delivery-only: `not started`, `partial`, `implemented`, or
`removed`.

## Proposal registry

| ID | Proposal | Status | Implementation |
| --- | --- | --- | --- |
| 0000 | [Minimal Runtime-Managed Memory](proposals/0000-minimal-memory-management.md) | accepted | implemented |
| 0001 | [Boolean Operators](proposals/0001-bool-operators.md) | accepted | implemented |
| 0002 | [Imports and Multi-File Packages](proposals/0002-imports-and-multi-file-packages.md) | accepted | implemented |
| 0003 | [Slices](proposals/0003-slices.md) | accepted | implemented |
| 0004 | [Enums / Tagged Unions](proposals/0004-enums-and-tagged-unions.md) | accepted | implemented |
| 0005 | [Pointers and Recursive Data](proposals/0005-pointers-and-recursive-data.md) | accepted | implemented |
| 0006 | [Basic String Operations](proposals/0006-basic-string-operations.md) | accepted | implemented |
| 0007 | [Maps](proposals/0007-maps.md) | accepted | implemented |
| 0008 | [Text and UTF-8 Helpers](proposals/0008-text-and-utf8-helpers.md) | accepted | implemented |
| 0009 | [Host Filesystem and Path Utilities](proposals/0009-host-filesystem-and-path-utilities.md) | accepted | implemented |
| 0010 | [Host Process and Environment](proposals/0010-host-process-and-environment.md) | accepted | implemented |
| 0011 | [Map Key Enumeration](proposals/0011-map-key-enumeration.md) | accepted | implemented |
| 0012 | [Sorting Helpers](proposals/0012-sorting-helpers.md) | accepted | implemented |
| 0013 | [Methods](proposals/0013-methods.md) | accepted | implemented |
| 0014 | [Generics](proposals/0014-generics.md) | accepted | implemented |
| 0015 | [Closures](proposals/0015-closures.md) | accepted | implemented |
| 0016 | [Interfaces](proposals/0016-interfaces.md) | accepted | implemented |
| 0017 | [Garbage Collection](proposals/0017-garbage-collection.md) | accepted | implemented |
| 0018 | [Error Comparison and Error Expressions](proposals/0018-error-comparison.md) | accepted | implemented |
| 0019 | [`to_str` Builtin](proposals/0019-to-str-builtin.md) | accepted | implemented |
| 0020 | [Testing Framework](proposals/0020-testing-framework.md) | accepted | implemented |
| 0021 | [Cross-Platform Build and Runtime](proposals/0021-cross-platform-build.md) | accepted | implemented |
| 0022 | [Dependency Management](proposals/0022-dependency-management.md) | accepted | implemented |
| 0023 | [TCP Networking](proposals/0023-tcp-networking.md) | accepted | implemented |
| 0024 | [Time Values and UTC](proposals/0024-time.md) | proposed | not started |
| 0025 | [Structured Concurrency](proposals/0025-concurrency.md) | accepted | implemented |
| 0026 | [Minimal HTTP Server](proposals/0026-http-server.md) | withdrawn | removed |
| 0027 | [HTTP Routing](proposals/0027-http-routing.md) | withdrawn | removed |
| 0028 | [Streaming Resource Model](proposals/0028-streaming-resource-model.md) | accepted | implemented |
| 0029 | [Struct Field Visibility and Package-Owned Construction](proposals/0029-struct-field-visibility.md) | accepted | implemented |
