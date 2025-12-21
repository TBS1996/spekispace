This file contains concepts and their constraints.

## Concepts:

### Class

A type of entity, not any specific entities.

e.g. "person" is a class, because it's an abstract concept.

### Instance

A specific instance of a given Class.

e.g., "albert einstein" is an instance of the Class "person".


### Attribute

a pre-defined question defined on a class that can be asked about all its instances

e.g. if  "birthdate" is an attribute on "person", when you create the "albert einstein" instance of person, it'll automatically ask the question of his birthdate, when you fill it out, this will be a new card.


### Parameter 

Helps identify a given instance of a class if the identifier name is not enough to disambiguate. 

For example, if you have a class "rust_function", and you want to add both the "get" function in ureq and in reqwest, it would be ambigious which rust function you're referring to. 

the "rust function" class can have a parameter "rust_crate" which specify what rust crate this function belongs to. 
So then you'll see rust_function<reqwest>::get, and rust_function<ureq>::get in the card so you know which "get" instance of rust_function youre referring to. 

conceptually a bit similar to attribute, but with an important difference. Parameters are for _identifying_ a specific instance, not asking a question about the instance. Parameters do not create separate cards like attributes do, and knowing about reqwest and ureq respectively are part of the dependencies for the "get" instance itself.

### Namespace

Namespaces are a way to disambiguate concepts when they only make sense in a given context. 

e.g., in kubernetes there are many concepts such as cluster, node, pod, etc..
these should all be represented as different classes. Instead of writing "kubernetes cluster", "kubernetes node", etc.. you can simply for each of them add "kubernetes" as a namespace and on the various class names just write cluster, node, pod. it'll show up as kubernetes::cluster when reviewing. The namespace must be a card, so that way you also get kubernetes as a dependency to all of those.

### Sets

A set is a grouping of cards, primarily used to review a certain topic. Unlike in anki, the sets don't "own" the cards, they merely reference them. Deleting a set does not delete the cards.

While you can explicitly list all cards belonging to a given set, primarily you will use the type system. You can define a set of "all instances of the class german noun" if you want to learn german for example. It uses set expressions with union, intersection, complement, difference, allowing you to represent any kind of advanced configuration.

### Recall

Recall is an estimate of the likelihood that at any given time you will be able to successfully review a given card, from 0% to 100% likelihood.

### Stability

Stability represents how stable the recall likelihood is over time. If you just learned a new fact you're likely to forget it soon. If you've reviewed it many times it'll probably take a long time for you to forget it. It is calculated by integrating the recall from the current time to the distant future.


## Constraints

- A class may have at most one parent class.
- An instance belongs to exactly one class.
- An instance is implicitly also an instance of all ancestor classes, recursively.
- A class defines a set of attributes.
- A class inherits all attributes from its ancestor classes.
- A class defines a set of parameters.
- A class inherits all parameters from its ancestor classes.
- The text of a card may contain links to other cards.
- Dependencies may be inferred implicitly from the type system and card contents.
- A card may declare explicit dependencies when implicit dependencies are insufficient.
- The dependency graph must be acyclic.
