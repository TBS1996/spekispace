# speki


Speki is a flashcard app.

PRs welcome!!

# about 

Speki is a flashcard app, which mainly takes inspiration from Anki, Supermemo, Obsidian, and Wikidata.

The backbone of Speki is it's dependency system. Each flashcard represents a piece of knowledge (as atomic as possible).
In order to understand almost any fact, you have to first understand a number of other facts. These are its _dependencies_.

No other flashcards app that i've seen currently ensures that you know the dependencies of a fact before it quizzes you on it. This often
leads to you being told to memorize things that you don't really understand. (read: [do not memorize before you understand](https://supermemo.guru/wiki/Do_not_memorize_before_you_understand)).

In speki, this is no longer a problem. All your cards live in a graph of dependencies, where on the top of the graph, you have foundational knowledge you need to learn first, and as you learn them, it "unlocks" other cards that depend on these. If you answer a card incorrecly which has other cards that depend on it, those dependents will be hidden from you until you've re-learned its dependencies. This means that when you answer a card incorrectly, it'll almost always be because you forgot the knowledge that the card itself represents, and not because you forgot a pre-requisite for knowing that card. 
