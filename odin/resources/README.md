# Retained resource roots

`Root` owns a directory handle and its diagnostic absolute path. Relative
operations traverse each path component through that handle; they do not reopen
the diagnostic path. Moving the selected directory and replacing its old pathname
therefore does not redirect reads, discovery, mutations or atomic publication.

Paths must be valid UTF-8 with `/` separators. Empty segments, `.`, `..`, drive
prefixes, alternate data streams, backslashes and NUL are rejected. Files must be
regular; discovery omits symlinks and Windows reparse points. Directory traversal
rejects links at every component. Recursive deletion removes nested links
themselves without following them; deleting an explicitly selected link is refused.

Darwin and Linux use descriptor-relative `openat`, `fstatat`, `renameat`,
`linkat` and `unlinkat`. Windows uses retained `RootDirectory` handles with
`NtCreateFile`, `NtQueryDirectoryFileEx` and `NtSetInformationFile`; child opens
include `FILE_OPEN_REPARSE_POINT` and verify the returned handle's attributes.

Writes first sync a unique sibling temporary file, then publish its handle with
an atomic rename. Exclusive creation refuses an existing destination. The returned
`published` flag is authoritative: an error after publication must not be described
as an unchanged file. Unix additionally syncs the parent directory after rename;
Windows flushes the file before handle-relative publication.

Reads are bounded by 64 MiB, discovery/deletion inventories by 20,000 entries,
and recursive traversal by 256 levels. Each result captures its allocator and has
an explicit destroy procedure. Application scene/model/script/audio code retains
the parent directory and allowed basename for an explicitly selected external file;
this package does not grant such capabilities implicitly.

Run `odin test odin/resources -vet -strict-style
-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true` on each native host. The portable
contract exercises create/replace/read/stat/list/search/delete after a root rename;
the Windows-specific contract creates an actual junction and checks rejection plus
recursive unlink without changing its outside target. Cross-compilation alone does
not establish those Windows filesystem behaviors.
