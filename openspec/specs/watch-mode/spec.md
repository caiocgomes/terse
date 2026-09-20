# Watch Mode Specification

## Purpose
Specifies `terse watch`: an initial normal build, transitive and attempted dependency tracking with a polling fallback, bounded debouncing and scheduling, preservation of the last successful generation on failure, dependency-graph refresh, and explicit attempt reporting and shutdown.

## Requirements

### Requirement: Watch performs an initial build using normal build contracts
`terse watch` SHALL select the entry/theme using normal project rules, perform an initial build, and then continue watching. It SHALL honor normal source-only/required-PDF behavior, validation, diagnostics, and publication contracts. Invalid startup configuration SHALL fail; a recoverable document error SHALL leave the process watching for repair even when no prior successful generation exists.

#### Scenario: Initial syntax error can be repaired
- **GIVEN** a valid manifest and a syntax error in the entry document
- **WHEN** watch starts and the source is subsequently corrected
- **THEN** the first attempt reports the source error and a later attempt publishes the first successful output without restarting watch

### Requirement: Watch tracks transitive and attempted dependencies
Watch SHALL observe the manifest, selected theme, lock/overrides, all transitive modules, referenced document/theme assets, and support files. It SHALL watch parent directories to handle atomic saves, deletion/recreation, and absent dependencies, with a polling fallback. Output/cache events MUST NOT trigger rebuild loops. After failure, it SHALL retain the union of last-successful and newly discovered/attempted dependencies; obsolete dependencies SHALL be pruned only after a successful rebuild.

#### Scenario: Atomic replacement and asset changes rebuild
- **WHEN** an editor atomically replaces an included file or a referenced logo/figure changes
- **THEN** watch detects the change and rebuilds using the new bytes

#### Scenario: Creating a missing include repairs the build
- **GIVEN** a failed attempt that introduced a reference to a missing module
- **WHEN** the module is created without another edit to the entry
- **THEN** watch detects its creation and retries the build

#### Scenario: Generated output does not cause a loop
- **WHEN** a successful build writes generated files and cache entries
- **THEN** those writes do not schedule another build in the absence of an input change

### Requirement: Debouncing and scheduling are bounded
Watch SHALL use a 150 ms trailing debounce window, run at most one build at a time, and keep at most one pending successor for changes arriving during an attempt. Each attempt SHALL use an input snapshot. Before publication, dependency changes SHALL make an attempt superseded so stale output is not published; a fresh attempt SHALL process the latest inputs.

#### Scenario: Burst of filesystem events
- **WHEN** multiple events for an edit arrive within the debounce window
- **THEN** watch coalesces them into one rebuild rather than starting overlapping processes

#### Scenario: Input changes while compilation runs
- **WHEN** a dependency changes during engine execution
- **THEN** that attempt is not published as current output and one successor processes the updated snapshot

### Requirement: Failed rebuilds preserve the last successful generation
Any source, include, symbol, citation, theme, asset, engine, or publication failure SHALL publish diagnostics only and leave the previous valid output intact. Successful correction SHALL rebuild automatically and transactionally replace that generation. An error MUST NOT clear the PDF or publish partial new source files.

#### Scenario: Watch after error [Acceptance K]
- **GIVEN** a successful watched build
- **WHEN** the author introduces a syntax error and later corrects it
- **THEN** the error points to the original source, every prior output byte remains intact during failure, and the correction triggers a successful replacement automatically

#### Scenario: Lock and engine failures also preserve output
- **WHEN** a referenced lock entry becomes stale or the engine fails during a rebuild
- **THEN** watch reports the failure, retains the previous artifacts, and continues watching for a repair

### Requirement: Configuration and dependency changes refresh the graph
Changes to the manifest, theme mapping, include graph, or reference/asset paths SHALL update watched dependencies using normal resolution rules. Failed graph updates MUST NOT discard the dependencies needed to detect a correction. Successful graph updates SHALL stop watching obsolete source dependencies except required parent-directory watches.

#### Scenario: New selected theme becomes active
- **WHEN** the manifest changes the selected theme's file mapping to another valid theme
- **THEN** the next build uses the new theme and its assets, and subsequent changes to those dependencies trigger rebuilds

### Requirement: Attempt reporting and shutdown are explicit
Watch SHALL report each completed attempt's success/failure/superseded status and publication result; JSON mode SHALL follow the diagnostics streaming contract. It SHALL continue after recoverable build failures. Explicit interruption SHALL stop scheduling work, terminate owned running child processes as needed, release locks, and preserve the last published generation.

#### Scenario: Interrupt during an attempt
- **WHEN** the user interrupts watch while a build is staging or compiling
- **THEN** watch exits without publishing partial output, leaves no owned compiler process running, and retains any previous valid generation
