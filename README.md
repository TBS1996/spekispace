# Speki

**Speki is an ontological flashcard app.**

It models knowledge explicitly, not as loose cards in decks, but as a structured system with well-defined dependencies. This leads to fundamentally different behavior from traditional flashcard apps.

## Core ideas

### Ontological cards
Cards are typed. They represent things like concepts, instances, attributes, and relations. Not just text prompts. Knowledge has structure.

### Knowledge as a directed acyclic graph
All cards live in a single DAG. Each card declares its dependencies. A card is only valid once its prerequisites are learned.

## What this enables

- **Prerequisite-safe reviews**  
  Speki will never ask a question if you have not learned all its transitive dependencies.  
  Example. You will not be asked for the gender of *perro* before learning what *perro* means.

- **No duplicate decks**  
  Traditional apps duplicate the same knowledge across countless decks.  
  Speki has one global knowledge graph. Each fact exists once.

- **Truly atomic cards**  
  Cards can be minimal and precise.  
  No need to repeat context. Dependencies guarantee the required background.

- **No hidden knowledge gaps**  
  Creating cards forces you to explicitly model prerequisites.  
  Missing foundations surface immediately instead of later during review.
