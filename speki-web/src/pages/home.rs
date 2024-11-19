use super::*;

#[component]
pub fn Home() -> Element {
    rsx! {
            div {
                class: "h-screen bg-gradient-to-br from-pink-400 to-purple-600 flex justify-center items-center",
                div {
                    class: "space-y-4 text-center",
                    button { class: "menu-item", "Home" }
                    button {
        class: "menu-item px-6 py-3 text-lg font-bold text-white bg-gradient-to-r from-blue-400 to-green-500 rounded-md shadow-lg transform hover:scale-105 active:scale-95 transition-transform",
        "Button Text"
    }
                    button { class: "menu-item", "About" }
                    button { class: "menu-item", "Services" }
                    button { class: "menu-item", "Contactar" }
                }
            }
        }
}
