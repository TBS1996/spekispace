# speki

https://getspeki.app/


Speki is a flashcard app.

PRs welcome!!

i would love to hear your thoughts in the [discussions](https://github.com/TBS1996/spekispace/discussions) section!


# about 

Speki is a flashcard app, which mainly takes inspiration from Anki, Supermemo, Obsidian, and Wikidata.

The backbone of Speki is it's dependency system. Each flashcard represents a piece of knowledge (as atomic as possible).
In order to understand almost any fact, you have to first understand a number of other facts. These are its _dependencies_.

No other flashcards app that i've seen currently ensures that you know the dependencies of a fact before it quizzes you on it. This often
leads to you being told to memorize things that you don't really understand. (read: [do not memorize before you understand](https://supermemo.guru/wiki/Do_not_memorize_before_you_understand)).

In speki, this is no longer a problem. All your cards live in a graph of dependencies, where on the top of the graph, you have foundational knowledge you need to learn first, and as you learn them, it "unlocks" other cards that depend on these. If you answer a card incorrecly which has other cards that depend on it, those dependents will be hidden from you until you've re-learned its dependencies. This means that when you answer a card incorrectly, it'll almost always be because you forgot the knowledge that the card itself represents, and not because you forgot a pre-requisite for knowing that card. 

As an example, you won't be asked what the powerhouse of the cell is, if speki believes you might have forgoten what the mitochondria is.


// todo: fix unstrucuted rambling below

From this fairly simple system comes countless of advantages that I've discovered in the process of making this app.
You no longer have to organize your cards into decks, they get naturally organized due to their innate properties. You can create collections, but
these are simply references to cards, the cards themselves don't belong to any collection. multiple collections can reference the same card.

speki also organizes cards into sophisticated ontology classes. you can have normal cards, classes, instances, and attributes.
a _class_ is any card that represent a concept in the world which might have specific _instances_  of itself. For example, you might have a class "german word". It can have sub-classes like "german noun" or "german verb". so you can have a card "haus" which is an instance of "german noun", or "lesen" as an instance of "german verb". these are also transitively instances of "german word". 

you can then define attributes, which are questions about the instances of a given class. for example, an attribute of "german noun" can be "what gender is {instance of german word}?". where you can specify that the answer has to be an instance of the glass "german grammatical gender".

Speki tries it's best to follow the DRY principle from programming. meaning, when making new cards, the user should not have to write information in the card that can be inferred from the card's properties. in other flashcard programs you'd have to write out Q: "what gender is the german word 'haus'?"  A: neuter.

In speki, you'd simply create a new instance of german noun, write the name "haus",be presented with an gender attribute, choose among the 3 options (instances of the class "german grammatical gender"), and a card will be generated with the full question and answer.


##  Speki's influences: Supermemo, Obsidian, Wikidata.

### Supermemo

Supermemo 