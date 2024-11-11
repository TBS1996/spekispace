        import * as git from "https://esm.sh/isomorphic-git@1.27.1";
        import http from "https://esm.sh/isomorphic-git@1.27.1/http/web";

export function greet(name) {
    console.log(`Hello, ${name} from JavaScript!`);
}



export async function clone_repo_and_list_files() {
    const output = document.getElementById("output");

    // Initialize BrowserFS with IndexedDB
    BrowserFS.configure({ fs: "IndexedDB", options: {} }, async (err) => {
        if (err) {
            output.textContent = "Failed to initialize BrowserFS: " + err;
            return;
        }

        const fs = BrowserFS.BFSRequire("fs");

        try {
            // Clone the repository with a CORS proxy
            await git.clone({
                fs,
                http,
                dir: "/my-other-repo",
                url: "https://github.com/tbs1996/talecast.git",
                corsProxy: "https://cors.isomorphic-git.org",
                singleBranch: true,
                depth: 1,
            });
            output.textContent = "Repository cloned successfully!";

            // List files in the root of the cloned repository
            fs.readdir("/my-other-repo", (err, files) => {
                if (err) {
                    output.textContent = "Error reading directory: " + err;
                    return;
                }

                output.textContent += "\nFiles in /my-other-repo:\n" + files.join("\n");
            });
        } catch (error) {
            output.textContent = "Failed to clone repository: " + error;
        }
    });
}
