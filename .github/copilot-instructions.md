# Speki Project Instructions

## Project Overview
**Speki is an ontological flashcard app** that models knowledge as a directed acyclic graph (DAG) rather than loose cards in decks. Unlike traditional flashcard apps, Speki represents knowledge with explicit structure and dependencies, ensuring prerequisite-safe reviews where you're never asked about something before learning its dependencies.

## Core Philosophy
- Knowledge has structure - cards are typed entities (concepts, instances, attributes) not just text prompts
- One global knowledge graph - each fact exists once, no duplicate decks
- Prerequisite-safe reviews - cards declare dependencies and form a DAG
- Atomic cards - cards can be minimal because dependencies provide context

## Architecture

### Core Components
- **speki-core**: Core library containing card logic, ledger system, and recall algorithms
- **speki**: Frontend application built with Dioxus (Rust UI framework)
- **ledgerstore**: Ledger storage and blockchain-like event system for history tracking
- **omtrent**: Time-related utilities

### Card Type System
- **Normal**: Standard flashcards with front and back
- **Instance**: Specific instance of a class (e.g., "Albert Einstein" is an instance of "Person")
- **Class**: Abstract type/category (e.g., "Person", "Rust Function")
- **Attribute**: Pre-defined questions on a class asked about all instances (e.g., "birthdate" attribute on "Person" class)
- **Statement**: Knowledge facts that can't easily be turned into questions
- **Unfinished**: Cards still being created
- **Event**: Time-based cards (dead code, ignore)

### Key Concepts

#### Class & Instance
- **Class**: Abstract concept/type (e.g., "person")
- **Instance**: Specific entity of a class (e.g., "Albert Einstein" of class "person")
- Classes can have parent classes (single inheritance)
- Instances implicitly belong to all ancestor classes

#### Attribute
Pre-defined questions on a class that automatically apply to all instances. When you answer an attribute for an instance, it creates a new card.
- Example: "birthdate" attribute on "person" class → automatically asks birthdate for each person instance
- Attributes are inherited from parent classes

#### Parameter
Used to disambiguate instances when the name alone isn't enough. Parameters _identify_ instances, they don't create separate cards.
- Example: `rust_function<reqwest>::get` vs `rust_function<ureq>::get` 
- The parameter (crate name) helps identify which "get" function you mean
- Parameters are inherited from parent classes

#### Namespace
Disambiguates concepts that only make sense in context. The namespace itself must be a card.
- Example: Instead of "kubernetes cluster", "kubernetes node" → just "cluster", "node" with namespace "kubernetes"
- Displays as `kubernetes::cluster` during review
- Namespace card becomes a dependency

#### Sets
Groups of cards for reviewing specific topics. Sets don't "own" cards, they reference them.
- Can explicitly list cards or use type system expressions (union, intersection, complement, difference)
- Example: "all instances of class german_noun" for learning German
- Deleting a set doesn't delete the cards

#### Dependencies
- Cards form a DAG - each card declares dependencies
- Dependencies can be implicit (from type system, card links) or explicit
- The dependency graph must be acyclic
- Speki only reviews cards whose transitive dependencies are learned

#### Recall & Stability
- **Recall**: Estimated likelihood (0-100%) you'll successfully review a card at any given time
- **Stability**: How stable recall is over time (calculated by integrating recall from now to distant future)

### Ledger System (ledgerstore)
The ledgerstore is a generic event-sourced persistence layer that provides:

#### Core Concepts
- **Event Sourcing**: All state changes are stored as immutable events in a blockchain-like structure
- **LedgerItem Trait**: Generic interface that any persistent item must implement
  - Defines `Key` (unique identifier), `Modifier` (mutation actions), `RefType` (types of references between items)
  - Implements `inner_run_event()` to apply modifications
  - Defines `ref_cache()` to declare references to other items
  - Implements `validate()` for invariant checking

#### Event Types
- **Create**: Add a new item to the ledger
- **Modify**: Apply a `Modifier` to an existing item (e.g., `CardAction` for cards)
- **Delete**: Remove an item (only if no other items depend on it)

#### Blockchain Structure
- Events are stored in a blockchain with cryptographic hashing
- Each `LedgerEntry` contains: previous hash, index, and the event
- Provides complete history and audit trail
- Located in `entries/` directory on disk

#### DAG Enforcement
- **Cycle Detection**: Automatically detects and rejects events that would create cycles
- **Dependency Tracking**: Maintains bidirectional indices (dependencies and dependents)
- **Cascade Validation**: When an item changes, all its dependents are re-validated
- Uses DFS to detect cycles before applying events

#### State Management
The ledger maintains multiple indices for efficient queries:
- **Items**: Canonical state of each item (in `state/items/`)
- **Properties**: Index items by property values (e.g., all cards of type "class")
- **Dependencies**: Index of item → items it depends on
- **Dependents**: Index of item → items that depend on it

#### Remote/Local Split
- **Local**: Working state stored locally
- **Remote**: Git-backed repository for sharing/syncing (uses git2)
- Can fetch and merge from upstream repositories
- Supports viewing remote cards without modifying them

#### Query System (ItemExpr)
Powerful expression system for querying sets of items:
- **Union**: Combine multiple sets
- **Intersection**: Items in all sets
- **Difference**: Items in first but not second
- **Complement**: All items except those in the set
- **Property**: All items with a given property value
- **Reference**: Items based on their dependencies/dependents (supports recursive traversal)

#### Caching
- In-memory cache (`Arc<RwLock<HashMap<Key, Arc<Item>>>`) for frequently accessed items
- Cache keys based on properties or item references
- Lazy loading - only loads items when needed

#### How Cards Use the Ledger
Cards implement `LedgerItem` with:
- `Key` = `CardId` (UUID)
- `Modifier` = `CardAction` enum (set front, set back, add dependency, etc.)
- `RefType` = `CardRefType` (explicit dependency, class of instance, parent class, etc.)
- All card operations go through `LedgerEvent::new_modify(card_id, action)`

### UI Structure
- Built with Dioxus (React-like framework for Rust)
- Uses Tailwind CSS for styling
- Overlay system for modals and card editing
- Card viewer/editor (`overlays/cardviewer/`) is the main interface for creating and editing cards

## Coding Conventions
- Use `Signal` for reactive state in Dioxus components
- Card operations go through the ledger system via `CardAction` events
- Use `APP.read()` to access the global application state
- Style classes defined in `speki/src/main.rs` under `styles` module (e.g., `CREATE_BUTTON`, `UPDATE_BUTTON`)
- Card type checking: use `card.is_class()`, `card.is_instance()`, etc.
