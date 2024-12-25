# speki

https://getspeki.app/


Speki is a flashcard app.

PRs welcome!!


for any feedback, use the issues page (even if it's not an actual issue)


# Developers guide

speki-web: The executable for the web-app
speki-core: The backend 
speki-dto: raw types and trait definitions
speki-provider: implementations of how speki saves to and loads from various storage systems
speki-cli: CLI version of speki, not maintained atm
speki-auth: Auth server for logging in with github, not working on atm as I use firestore
speki-proxy: Cors server also needed with github integration, also not working on this atm

The speki-web is where the main work goes.

it's a dioxus application, atm im developing it for the web as a PWA. In the future i'll create the desktop and mobile apps too. Although PWA works as a mobile app anyway. 


