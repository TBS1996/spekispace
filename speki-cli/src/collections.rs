use crate::utils::{get_input_opt, select_item};
use speki_core::collections::{add, commit, push, Collection};

pub fn col_stuff() {
    loop {
        let mut opts: Vec<String> = Collection::load_all()
            .iter()
            .map(|col| col.name().to_string())
            .collect();
        opts.insert(0, "exit".to_string());
        opts.insert(0, "create new".to_string());
        let selection = select_item(&opts);

        match selection {
            0 => {
                if let Some(name) = get_input_opt("name of collection") {
                    Collection::create(&name);
                }
            }
            1 => {
                return;
            }
            num => {
                let col = Collection::load_all().remove(num - 2);
                manage_col(col);
            }
        }
    }
}

fn manage_col(col: Collection) {
    let repo = &col.repo;
    let opts = ["pull", "push", "return"];

    match select_item(&opts) {
        0 => {
            col.pull();
        }
        1 => {
            // col.pull();
            add(repo);
            commit(repo).unwrap();
            push(repo).unwrap();
        }
        2 => {}
        _ => panic!(),
    }
}
