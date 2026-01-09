# Speki Card Creation Guide for LLMs

This guide explains how to create flashcards from knowledge sources (articles, documents, etc.) using the Speki CLI. Speki uses an ontological approach where knowledge is structured as a directed acyclic graph (DAG) with typed entities and dependencies.

**Important:** Before creating cards, read the `best_practices` file for essential guidelines on card formulation and structure (namespaces, atomic cards, avoiding cycles, etc.). This guide focuses on CLI usage and LLM-specific workflows.

## Core Concepts

### Card Types

1. **Class** - Abstract concepts/categories (e.g., "programming language", "person", "chemical element")
2. **Instance** - Specific entities of a class (e.g., "Rust" is an instance of "programming language")
3. **Attribute** - Pre-defined questions on a class that automatically apply to all instances (e.g., "birthdate" on "person")
4. **Normal** - Standard flashcards with front and back
5. **Statement** - Knowledge facts that can't easily be turned into questions
6. **Unfinished** - Cards still being created

### Key Ontology Features

- **Classes can have parent classes** (single inheritance) - instances inherit attributes from ALL ancestor classes recursively
- **Attribute inheritance** - if class A has parent B, and B has parent C, instances of A will have attributes from A, B, and C
- **Dependencies** form a DAG - you can't review a card before learning its dependencies
- **Attributes** create cards automatically - when you add an attribute to a class, you can then answer it for each instance (including instances of child classes)

**See the `best_practices` file for detailed guidance on:**
- Using namespaces for disambiguation
- Keeping cards atomic
- Avoiding "normal" cards when possible
- Minimizing explicit dependencies
- Avoiding cycles in the dependency graph
- Using text references to link cards

### Dependencies

Dependencies are automatically inferred from:
- Instance → Class relationship
- Class → Parent class relationship
- Namespace card references
- Attribute → Instance relationship
- **Card links in front/backside text** - When you reference another card using `[[card-id]]` syntax, that card automatically becomes a dependency

You can also add explicit dependencies between any cards, but prefer using card links to minimize explicit dependencies.

### Using Card Links to Minimize Dependencies

**When referring to other cards in question or answer text, use card links instead of plain text.** This creates automatic dependencies and makes the knowledge graph more connected.

**Bad approach (plain text):**
```
Front: "Which country is to the south of Macedonia?"
```

**Good approach (with card link):**
```
Front: "Which country is to the south of [[<macedonia-card-id>]]?"
```

**Benefits:**
1. Automatic dependency - You won't see this card until you've learned the Macedonia card
2. Interactive links during review - You can navigate to related cards
3. More precise references - No ambiguity about which "Macedonia" you mean
4. Less need for explicit dependencies

**When the answer is a single card, use `CardRef` backside type:**

```bash
# Instead of {"Text": "Greece"}, use CardRef if Greece is a card:
speki-app --action '{
  "NormalType": {
    "front": "Which country is south of [[<macedonia-id>]]?",
    "back": {"CardRef": "<greece-card-id>"}
  }
}'
```

This means:
- The answer displays as a link to the Greece card
- Greece card becomes a dependency
- You can navigate directly to Greece card from the review
- No need to manually add Greece as an explicit dependency

### Leveraging the Class System for Disambiguation

**Keep frontside text minimal when the class system provides context.** During reviews, the card's type information is displayed, so you don't need to repeat it in the front text.

**Bad approach (redundant information):**
```
Instance of type "ethnicity": "ethnic Macedonian"
```

**Good approach (class provides context):**
```
Instance of type "ethnicity": "Macedonian"
```

During review, Speki shows you're reviewing an instance of "ethnicity", so "ethnic" is redundant. The user sees something like:

```
[ethnicity instance]
Macedonian
```

**When to be more specific:**
- Only when the minimal name is genuinely ambiguous even with class context
- When there are multiple instances with the same name in the same class (use parameters instead)

**Examples of good minimal naming:**
- Instance of "programming_language": "Rust" (not "Rust programming language")
- Instance of "country": "Macedonia" (not "Republic of Macedonia")  
- Instance of "chemical_element": "Carbon" (not "Carbon element")
- Instance of "spanish_noun": "casa" (not "Spanish word casa")

The class system already tells you what type of thing it is, so keep the instance name clean and minimal.

## CLI Usage

### Search for Existing Cards

Before creating cards, search to avoid duplicates:

```bash
# Search by text content
speki-app --load-cards --contains "rust" --format json

# Search by card type
speki-app --load-cards --card-type class --format json
speki-app --load-cards --card-type instance --format json

# Combine filters
speki-app --load-cards --card-type instance --contains "rust" --format json

# Limit results
speki-app --load-cards --contains "programming" --limit 10
```

**Inspect a class to see all its attributes (including inherited):**

```bash
# Get complete class info including all attributes from parent classes
speki-app --card "<class-id>" --show-class-info
```

This outputs JSON with:
- `id`: The class ID
- `name`: The class name
- `parent_class`: Parent class ID (if any)
- `attributes`: Array of all attributes with `inherited: true/false` flag

Example output:
```json
{
  "id": "abc-123...",
  "name": "scientist",
  "parent_class": "def-456...",
  "attributes": [
    {
      "id": "attr-1",
      "pattern": "name",
      "back_type": null,
      "inherited": true
    },
    {
      "id": "attr-2",
      "pattern": "birthdate",
      "back_type": {"TimeStamp": null},
      "inherited": true
    },
    {
      "id": "attr-3",
      "pattern": "field_of_study",
      "back_type": null,
      "inherited": false
    }
  ]
}
```

**Output formats:**
- `--format text` (default): Human-readable Q&A format
- `--format json`: Full card data including ID, type, dependencies, etc.
- `--format id`: Just the card IDs

### Create Cards with JSON Actions

The `--action` flag accepts JSON matching the `CardAction` enum.

**Organizing Cards into Sets:**
- Use `--set <name>` to automatically add newly created cards to a specific set
- The set ID is generated deterministically from the name (same name = same set)
- If the set doesn't exist, it will be created automatically
- This makes LLM card creation idempotent and organized by source
- Example: `--set "rust_ownership_article"` will create/use a set for that article

**When `--card <ID>` is optional (creates new card if omitted):**
- `ClassType` - Creates a new class
- `InstanceType` - Creates a new instance
- `NormalType` - Creates a new normal card
- `StatementType` - Creates a new statement
- `UnfinishedType` - Creates a new unfinished card

**When `--card <ID>` is REQUIRED (modifies existing card):**
- All other actions modify existing cards and require `--card` to specify which card to modify
- Examples: `SetFront`, `SetBackside`, `AddDependency`, `RemoveDependency`, `SetNamespace`, `SetParentClass`, `SetInstanceClass`, `AttributeType`, etc.

**Special Commands:**
- Use `--add-attribute` (requires `--card`) to add attributes to classes (generates attribute IDs automatically)
- Do NOT use `InsertAttr` or `SetAttrs` via `--action` - they will be rejected

#### Creating a Class

```bash
speki-app --action '{
  "ClassType": {
    "front": "programming language"
  }
}'
```

This outputs the new card's UUID. Save it for creating instances!

#### Creating an Instance

```bash
# First, get the class ID (search or from previous creation)
CLASS_ID="<uuid-of-programming-language-class>"

speki-app --action '{
  "InstanceType": {
    "front": "Rust",
    "class": "'$CLASS_ID'"
  }
}'
```

#### Creating a Normal Card

```bash
speki-app --action '{
  "NormalType": {
    "front": "What is the time complexity of binary search?",
    "back": {"Text": "O(log n)"}
  }
}'
```

#### Adding Attributes to a Class

Attributes are questions that apply to all instances of a class. Use the dedicated `--add-attribute` command which automatically generates the attribute ID.

**Before Creating an Attribute:**
- **Check if it already exists on parent classes** - Attributes are inherited, so don't duplicate them
- Example: If adding "birthdate" to "Scientist" class, first check if "Person" (parent) already has it
- Use `--card <class-id> --show-class-info` to see ALL attributes for a class including inherited ones
- Look for `"inherited": true` in the output to see which attributes come from parent classes
- Only add an attribute if it's specific to this class level and not already inherited

**Naming Guidelines:**
- Keep attribute names **minimal** - use single words or short phrases
- Good: `"birthdate"`, `"capital"`, `"atomic_number"`, `"gender"`
- Bad: `"When was this person born?"`, `"What is the capital of this country?"`

**Type Constraints:**
- **Always use typed `back_type` when possible** - this enforces correct answer types
- `{"TimeStamp": null}` - For dates/timestamps (birthdate, founding_date, etc.)
- `{"Boolean": null}` - For yes/no questions (is_nullable, is_mutable, etc.)
- `{"InstanceOfClass": "class-id"}` - To restrict answer to instances of a specific class (e.g., gender must be instance of "spanish_gender" class)
- `null` - Only when answer is free-form text with no constraints

```bash
CLASS_ID="<uuid-of-person-class>"

# Add a typed attribute and capture its ID
ATTR_ID=$(speki-app --card "$CLASS_ID" --add-attribute '{
  "pattern": "birthdate",
  "back_type": {"TimeStamp": null}
}')

echo "Created attribute with ID: $ATTR_ID"
```

**Example with InstanceOfClass constraint:**
```bash
SPANISH_NOUN_CLASS="<uuid-of-spanish-noun-class>"
SPANISH_GENDER_CLASS="<uuid-of-spanish-gender-class>"

# Gender attribute must reference an instance of spanish_gender class
GENDER_ATTR=$(speki-app --card "$SPANISH_NOUN_CLASS" --add-attribute '{
  "pattern": "gender",
  "back_type": {"InstanceOfClass": "'$SPANISH_GENDER_CLASS'"}
}')
```

**Important:** 
- Always use `--add-attribute` instead of `InsertAttr` or `SetAttrs` in `--action`
- The attribute ID is auto-generated and returned by the command
- You'll need this ID when answering the attribute for instances
- **Attributes are inherited**: If class B extends class A, instances of B inherit all attributes from A

**Inheritance Example:**
```
Class hierarchy: Thing → Person → Scientist
- "Thing" has attribute: "name"
- "Person" has attribute: "birthdate"  
- "Scientist" has attribute: "field_of_study"

An instance of "Scientist" (e.g., Albert Einstein) will have ALL three attributes:
- name (from Thing)
- birthdate (from Person)
- field_of_study (from Scientist)
```

#### Answering an Attribute for an Instance

Once a class has an attribute, answer it for specific instances using the attribute ID. This creates a new attribute card, so `--card` should specify the instance card:

```bash
INSTANCE_ID="<uuid-of-rust-creator-instance>"
ATTR_ID="<uuid-of-birthdate-attribute>"

speki-app --card "$INSTANCE_ID" --action '{
  "AttributeType": {
    "attribute": "'$ATTR_ID'",
    "back": {"Time": "1990-01-01T00:00:00Z"},
    "instance": "'$INSTANCE_ID'"
  }
}'
```

Note: 
- The `attribute` field uses the UUID returned when you created the attribute with `--add-attribute`
- The `instance` field should match the `--card` parameter (the instance you're answering about)
- This creates a new card for the attribute answer

#### Setting a Namespace

```bash
NAMESPACE_ID="<uuid-of-kubernetes-card>"
CARD_ID="<uuid-of-cluster-card>"

speki-app --card "$CARD_ID" --action '{
  "SetNamespace": "'$NAMESPACE_ID'"
}'
```

#### Adding Explicit Dependencies

```bash
CARD_ID="<uuid-of-dependent-card>"
DEPENDENCY_ID="<uuid-of-prerequisite-card>"

speki-app --card "$CARD_ID" --action '{
  "AddDependency": "'$DEPENDENCY_ID'"
}'
```

#### Setting Parent Class

```bash
CHILD_CLASS_ID="<uuid-of-systems-programming-language>"
PARENT_CLASS_ID="<uuid-of-programming-language>"

speki-app --card "$CHILD_CLASS_ID" --action '{
  "SetParentClass": "'$PARENT_CLASS_ID'"
}'
```

#### Creating a Statement

For facts that aren't easily turned into questions:

```bash
speki-app --action '{
  "StatementType": {
    "front": "The speed of light in vacuum is exactly 299,792,458 meters per second."
  }
}'
```

### Modifying Existing Cards

To modify a card, use `--card <ID>` with an action:

```bash
# Change the front text
speki-app --card "<card-id>" --action '{
  "SetFront": "Updated question text"
}'

# Change the backside
speki-app --card "<card-id>" --action '{
  "SetBackside": {"Text": "Updated answer"}
}'

# Add a dependency
speki-app --card "<card-id>" --action '{
  "AddDependency": "<other-card-id>"
}'

# Remove a dependency
speki-app --card "<card-id>" --action '{
  "RemoveDependency": "<other-card-id>"
}'
```

## Workflow for Processing a Knowledge Source

### 0. Create a Plan (IMPORTANT - Do This First!)

Before creating any cards, **research existing cards and create a text file with your execution plan**. All searching and decision-making happens during planning - the execution plan should only contain concrete actions.

**Planning Process:**

1. **Analyze the source material** - Identify concepts, classes, instances, attributes, relationships
2. **Search for existing cards** - Use `--load-cards` with various filters to find what already exists
3. **Make decisions** - Determine which cards to create, which to reuse, which attributes exist on parent classes
4. **Write the execution plan** - Document the concrete actions you'll take with no ambiguity

**Create a file named `card_creation_plan.txt` or similar with:**

#### Plan Structure

The plan should have three main sections:

1. **Research Summary** - What you found when searching existing cards
2. **Overview** - High-level summary of what you're creating
3. **Execution Steps** - Detailed, ordered list of every action to perform (no searching, no conditionals)

#### Research Summary Section

Document what you found during your searches:

```
## Research Summary

Searched for existing cards:
- "programming_language" class: FOUND (ID: abc-123...)
- "Rust" instance: NOT FOUND - will create
- "person" class: FOUND (ID: def-456...)
  - Has attributes: name (inherited from "thing"), birthdate, nationality
- "scientist" class: NOT FOUND - will create
  - Will inherit name, birthdate, nationality from "person"
  - Will add: field_of_study (specific to scientist)
```

**This section shows:**
- What cards already exist (with their IDs)
- What cards need to be created
- What attributes are already available on classes (including inherited ones)
- What attributes you'll add (only those not already inherited)

#### Overview Section

Include:

- **Set Name** - Choose a descriptive name for this import
  - **If the source text/file specifies a set name, use that exactly**
  - Otherwise, create a descriptive name yourself (e.g., the filename, article title, or topic)
  - All cards will be grouped into this set

- **Class Hierarchy** - List all classes and their parent relationships (including existing ones)
  ```
  Example:
  - thing (EXISTING: xyz-789...)
    - person (EXISTING: def-456...)
      - scientist (NEW - will create)
  ```

- **Summary** - Brief description of what knowledge is being modeled

#### Execution Steps Section

**CRITICAL RULES:**
1. **No searching or conditionals** - Don't say "check if X exists" or "search for Y". You already did that during planning!
2. **Every step must be a concrete action** - Only include definitive operations like "Create class" or "Set namespace"
3. **Use existing IDs directly** - If a card exists, reference it by its actual UUID (from your research)
4. **Every step must be in correct dependency order** - Never reference a card before it's created
5. **Use `$N` notation for newly created cards** - `$1` refers to the UUID from step 1, `$2` from step 2, etc.
6. **Track execution progress** - Mark each step as `[ ]` (pending), `[in progress]`, or `[done: uuid]`

**Step Format:**
```
[status] Step N: Action description
  Type: <card-type or action>
  Details: <key parameters>
  Returns: <what ID this step produces>
  Notes: <optional context>
```

**Example plan file:**
```
# Card Creation Plan for Rust Ownership Article

## Research Summary

Searches performed:
1. Search for "programming_language" class:
   - FOUND: ID abc-def-123...
   - Has attributes: release_year, paradigm (both directly on this class)

2. Search for "Rust" instance:
   - NOT FOUND - will create as instance of programming_language

3. Search for "memory_management_concept" class:
   - NOT FOUND - will create

4. General search for "ownership", "borrowing", "lifetime":
   - NOT FOUND - will create as instances

## Overview

### Set Name
rust_ownership_article

### Class Hierarchy
- programming_language (EXISTING: abc-def-123...)
- memory_management_concept (NEW - will create)

### Summary
Creating cards about Rust's ownership system. Reusing existing "programming_language" class
and its attributes. Creating new "memory_management_concept" class for ownership-related concepts.
Rust instance will answer existing release_year attribute from programming_language class.

## Execution Steps

[ ] Step 1: Create "memory_management_concept" class
  Type: ClassType
  Front: "memory management concept"
  Returns: class ID → $1

[ ] Step 2: Create "Rust" instance
  Type: InstanceType
  Front: "Rust"
  Class: abc-def-123... (existing programming_language class)
  Returns: instance ID → $2

[ ] Step 3: Answer "release_year" attribute for Rust
  Type: AttributeType (on card $2)
  Attribute: existing-attr-id-456... (release_year from programming_language)
  Instance: $2 (Rust)
  Back: {"Time": "2010-01-01T00:00:00Z"}
  Returns: attribute card ID → $3

[ ] Step 4: Create "ownership" instance
  Type: InstanceType
  Front: "ownership"
  Class: $1 (memory_management_concept)
  Returns: instance ID → $4

[ ] Step 5: Set namespace for ownership
  Type: SetNamespace (on card $4)
  Namespace: $2 (Rust instance)
  Returns: nothing

[ ] Step 6: Create normal card about ownership rules
  Type: NormalType
  Front: "What are the three ownership rules?"
  Back: {"Text": "1. Each value has an owner. 2. Only one owner at a time. 3. Value dropped when owner goes out of scope."}
  Returns: card ID → $6

[ ] Step 7: Add dependency to ownership rules card
  Type: AddDependency (on card $6)
  Dependency: $4 (ownership instance)
  Returns: nothing

## Execution Log

(Will be filled in as execution proceeds)
```
Rust as an instance, and specific ownership-related concepts. Notice how the research
happens first, then the overview summarizes findings, and execution steps are concrete
actions with no conditionals or searches.

**After creating the plan:**
- Show it to the user
- Wait for approval/feedback
- Make any requested changes
- Only then proceed with execution, updating the status as you go
- Track all returned UUIDs in the execution log

### 1. Analyze the Source (Part of Planning)

Read through the document and identify:
- Abstract concepts (potential classes)
- Specific entities (potential instances)
- Properties/questions that apply to multiple entities (potential attributes)
- Facts and relationships (normal cards, statements, dependencies)
- Contextual groupings (potential namespaces)

### 2. Check for Existing Cards (Part of Planning)

For each concept, search to see if it already exists:

```bash
speki-app --load-cards --contains "rust programming" --format json
```

Parse the JSON output to extract existing card IDs and avoid duplicates. This research goes into the "Research Summary" section of your plan.

### 3. Build the Ontology Bottom-Up (Execution Phase)

Once your plan is approved, execute the steps in dependency order:

1. **Create classes first** (no dependencies)
2. **Set up class hierarchy** (parent classes)
3. **Add attributes to classes** (check parent classes first to avoid duplicates - attributes are inherited!)
4. **Create instances** (reference their classes)
5. **Answer attributes for instances** (including inherited ones from parent classes)
6. **Create normal cards** with explicit dependencies as needed
7. **Add namespace relationships** for context

**Note:** All the decision-making about what already exists and what needs to be created was done during planning. Execution should be mechanical and follow the plan.

### 4. Establish Dependencies (Execution Phase)

Execute the dependency actions from your plan:
- If understanding card A requires knowing card B, add B as a dependency of A
- Classes are automatically dependencies of their instances
- Don't create circular dependencies (the system will reject them)

### 5. Use Namespaces for Context (Execution Phase)

When concepts only make sense in a specific context, use namespaces instead of prefixing names:

```bash
KUBERNETES_ID="<uuid-of-kubernetes-card>"
CLUSTER_ID="<uuid-of-cluster-card>"

speki-app --card "$CLUSTER_ID" --action '{
  "SetNamespace": "'$KUBERNETES_ID'"
}'
```

This makes the card display as `kubernetes::cluster` during review.

## Example: Processing a Rust Article

This example shows the complete workflow including the planning phase.

### Planning Phase

Given an article about Rust's ownership system:

**Step 1: Identify Structure**

- **Class**: "programming language", "memory management concept"
- **Instances**: "Rust", "ownership", "borrowing", "lifetime"
- **Attributes**: For "programming language" - "release year", "paradigm"
- **Normal cards**: "What are the three ownership rules?", "What's the difference between `&` and `&mut`?"

**Step 2: Research Existing Cards**

```bash
# Check if "programming language" class exists
speki-app --load-cards --card-type class --contains "programming language" --format json
# Result: FOUND with ID abc-123..., has attributes release_year and paradigm

# Check if "Rust" instance exists
speki-app --load-cards --card-type instance --contains "rust" --format json
# Result: NOT FOUND

# Check for memory management concepts
speki-app --load-cards --contains "memory management" --format json
# Result: NOT FOUND
```

**Step 3: Create Execution Plan**

Write a plan file documenting:
- Research findings (what exists, what doesn't)
- Overview of what you'll create
- Concrete execution steps with no conditionals

(See the example plan file in section "0. Create a Plan" above)

### Execution Phase

Once the plan is approved, execute the steps mechanically:

```bash
# Step 1: Create "memory management concept" class (from plan)
MMC_CLASS=$(speki-app --action '{"ClassType":{"front":"memory management concept"}}')

# Step 2: Create "Rust" instance using EXISTING programming_language class
RUST=$(speki-app --action '{"InstanceType":{"front":"Rust","class":"abc-123..."}}')

# Step 3: Answer release_year attribute using EXISTING attribute ID
speki-app --card "$RUST" --action '{"AttributeType":{"attribute":"existing-attr-id...","back":{"Time":"2010-01-01T00:00:00Z"},"instance":"'$RUST'"}}'

# Step 4: Create "ownership" instance
OWNERSHIP=$(speki-app --action '{"InstanceType":{"front":"ownership","class":"'$MMC_CLASS'"}}')

# Step 5: Set namespace
speki-app --card "$OWNERSHIP" --action '{"SetNamespace":"'$RUST'"}'

# Step 6: Create normal card about ownership rules
RULES=$(speki-app --action '{
  "NormalType":{
    "front":"What are the three ownership rules in Rust?",
    "back":{"Text":"1. Each value has an owner. 2. There can only be one owner at a time. 3. When the owner goes out of scope, the value is dropped."}
  }
}')

# Step 7: Add dependency
speki-app --card "$RULES" --action '{"AddDependency":"'$OWNERSHIP'"}'
```

Notice how during execution:
- No searching happens - we already know what exists
- We use existing IDs directly (abc-123... for programming_language class)
- We create new cards for what doesn't exist
- All decisions were made during planning

## Tips for LLMs

1. **Research first, plan second, execute third** - The workflow is: (1) Search for existing cards, (2) Document findings and write execution plan, (3) Execute the plan mechanically
2. **Execution plans should have no conditionals** - Don't say "if X exists then..." in execution steps. You already figured that out during research!
3. **Use existing IDs directly in plans** - When a card exists, put its actual UUID in the execution steps
4. **Parse JSON output carefully** - The `--format json` output contains all card metadata including attributes and their IDs
5. **Check parent classes for attributes** - Before adding an attribute to a class, verify it doesn't already exist on any parent class (attributes are inherited!)
6. **Build incrementally** - Create classes, then instances, then relationships
7. **Think about learning order during planning** - Dependencies should reflect prerequisite knowledge
8. **Keep attribute names minimal** - Use `"birthdate"` not `"When was this person born?"`
9. **Type attribute answers** - Use `TimeStamp`, `Boolean`, or `InstanceOfClass` constraints whenever possible, not just `null`
10. **Use `--add-attribute` for attributes** - Never try to specify attribute IDs manually via `--action`
11. **Save all returned IDs** - Store UUIDs returned by card creation and attributes for later reference
12. **Follow best practices** - See `best_practices` file for principles on card structure, namespaces, atomic cards, etc.
13. **Show your plan before executing** - Always get user approval on the execution plan before creating any cards
14. **Use card links `[[id]]` liberally** - When referring to other cards in question/answer text, use `[[card-id]]` or `[[card-id|alias]]` syntax instead of plain text. This creates automatic dependencies and reduces need for explicit `AddDependency` actions
15. **Use `CardRef` backsides when appropriate** - If the answer is simply another card (not descriptive text), use `{"CardRef": "card-id"}` instead of `{"Text": "..."}`. This creates a navigable link and automatic dependency
16. **Keep instance names minimal** - The class system provides context during review, so don't repeat type information in the instance name. Use "Macedonian" not "ethnic Macedonian" when it's an instance of "ethnicity". Use "Rust" not "Rust programming language" when it's an instance of "programming_language"

## JSON Structure Reference

### TextData Format

`TextData` is simply a string that can contain card references using wiki-style link syntax:

```json
"What is the capital of France?"
```

You can embed references to other cards within the text:

```json
"What is the capital of [[a1b2c3d4-e5f6-7890-abcd-ef1234567890]]?"
```

Or with an alias for better readability:

```json
"What is the capital of [[a1b2c3d4-e5f6-7890-abcd-ef1234567890|France]]?"
```

**Link Syntax:**
- `[[card-id]]` - Embeds a link to another card (displays the card's front text)
- `[[card-id|alias]]` - Embeds a link with custom display text

When you embed a card reference using `[[id]]`, that card automatically becomes a dependency. This is useful for building interconnected knowledge where understanding one concept requires knowing another.

**Example with embedded references:**
```bash
FRANCE_ID="<uuid-of-france-card>"

speki-app --action '{
  "NormalType": {
    "front": "What is the capital of [['$FRANCE_ID'|France]]?",
    "back": {"Text": "Paris"}
  }
}'
```

This creates a card where the France card becomes an implicit dependency.

### BackSide Formats

The answer side of a card can be one of several types:

```json
{"Text": "answer text"}
{"Bool": true}
{"Time": "2024-01-01T00:00:00Z"}
{"CardRef": "card-uuid-here"}
```

Note: `Text` backsides also support the `[[id]]` and `[[id|alias]]` link syntax for embedding card references.

### Common Actions

**Create cards:**
- `ClassType` - New class
- `InstanceType` - New instance  
- `NormalType` - New normal card
- `StatementType` - New statement
- `AttributeType` - Answer an attribute for an instance

**Modify cards:**
- `SetFront` - Change question
- `SetBackside` - Change answer
- `AddDependency` / `RemoveDependency` - Manage dependencies
- `SetNamespace` - Set/unset namespace
- `SetParentClass` - Set class hierarchy
- `SetInstanceClass` - Change instance's class

**Note:** Use `--add-attribute` command instead of `InsertAttr` or `SetAttrs` in `--action`

## Error Handling

- If a card creation fails, the CLI will print an error and exit with code 1
- The error message will indicate what went wrong (e.g., "cycle detected", "card not found")
- Always validate that referenced cards exist before creating dependencies
- Ensure class IDs are valid before creating instances

## All Created Cards Go to "CLI imports" Set

Any card created via `--action` (when no `--card` is specified) is automatically added to a set called "CLI imports". You can review this set to see all cards created via CLI.
