# FellaDB

FellaDB is a proposed optional companion app for personal filesystem search.
It gives Fella's General conversations an explicit way to find and ask about
files beyond a repository workspace, while leaving Fella's analytics harness
and its current workspace tools intact.

This is product direction, not shipped behavior. Fella works fully without
FellaDB.

## Product boundary

FellaDB owns broad personal-file discovery, indexing, and search. Fella remains
the conversational analytics app. When FellaDB is installed, Fella can use it
from General to search the user's selected personal-file scope and ground an
answer in the returned files. Fella can also open the FellaDB app when the user
wants to manage that scope or its index.

FellaDB is separate from repository workspaces. A General conversation may use
personal files only when the user explicitly selects that scope. It must not
inherit the currently focused repository, add personal files to a repository
catalog, or change the workspace identity of the conversation. Repository
workspaces continue using Fella's existing catalog and analysis tools.

The initial search-engine direction is FFF only. FellaDB should use FFF for
file-name, path, and content search rather than maintaining a competing search
engine. Embeddings are not required for the initial product to work; any future
semantic layer would be optional and additive to FFF.

## Companion discovery and use

Fella should detect an installed FellaDB through a stable installation
location or registration shared by the FellaDB and Fella installers. Users
should not have to configure an executable path, edit `PATH`, or manually
connect the apps. If FellaDB is absent or incompatible, General remains
available and can explain how to install or update the companion.

Fella should communicate with FellaDB through a narrow local interface for
availability, scope status, search, and source excerpts. FellaDB owns its index
and indexing lifecycle. Fella may launch or connect to its search process when
needed, and open its UI for library management; ordinary questions should not
require the user to switch apps. The exact process model, IPC transport,
installer ownership, and version-compatibility policy remain implementation
decisions.

## Scope, privacy, and evidence

- The user chooses the locations FellaDB can search. Selected folders are the
  starting point; whole-disk search is a deliberate expansion, not a default.
- Finding the companion must not silently grant filesystem access. FellaDB
  reports the locations and file types indexed, exclusions, inaccessible
  areas, and whether the initial scan or updates are still in progress.
- Search is read-only. Fella receives ranked matches with paths and relevant
  excerpts, then cites those sources in its General answer. It should not imply
  that files were searched when their contents were not indexed or accessible.
- The index stays on the user's device. Local indexing does not make model
  inference local: excerpts sent from Fella to the user's selected model
  provider are subject to Fella's existing provider data flow and must be
  disclosed as such.
- FellaDB's local library, index, and scope are separate from Fella's
  repository-scoped catalog, memory, and analysis history.

## Longer-term direction

FellaDB may later provide search and read primitives to Fella's existing
filesystem exploration tools. That would be an implementation option, not a
prerequisite for the companion or a reason to replace Fella's catalog, SQL,
Python, or analytical runtime now. Any such integration must preserve explicit
scope and source provenance.

Before implementation, validate FFF for the intended personal-file scopes and
platforms, including whole-disk access, indexing coverage, resource use, and
packaged-app discovery. The first supported operating systems, default
exclusions, update channel, and any semantic-search need are not decided here.
