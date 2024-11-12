import * as git from "https://esm.sh/isomorphic-git@1.27.1";
import http from "https://esm.sh/isomorphic-git@1.27.1/http/web";

export function greet(name) {
    console.log(`Hello, ${name} from JavaScript!`);
}

export async function listFiles(path) {
    const output = document.getElementById("output");
    // Initialize BrowserFS with IndexedDB
    BrowserFS.configure({ fs: "IndexedDB", options: {} }, async (err) => {
        if (err) {
            output.textContent = "Failed to initialize BrowserFS: " + err;
            return;
        }

        const fs = BrowserFS.BFSRequire("fs");

        console.log(`listing fles from, ${path}!`);

        try {
            // List files in the root of the cloned repository
            fs.readdir(path, (err, files) => {
                if (err) {
                    output.textContent = "Error reading directory: " + err;
                    return;
                }

                output.textContent += "\nFiles in /my-other-repo:\n" + files.join("\n");
            });
        } catch (error) {
            output.textContent = "Failed to read repository: " + error;
        }
    });
}

export async function cloneRepo(path, url, token) {
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
        console.log(`starting clone from ${url} fles to ${path}!`);
            await git.clone({
                fs,
                http,
                dir: path, //"/my-other-repo",
                url: url, //
                corsProxy: "https://cors.isomorphic-git.org",
                singleBranch: true,
                depth: 1,
                onAuth: () => ({
                    username: 'x-access-token',
                    password: token
                })
            });
            console.log(`successsddd nice`);
            output.textContent = "Repository cloned successfully!";
        } catch (error) {
            output.textContent = "Failed to clone repository: " + error;
        }
    });
}
