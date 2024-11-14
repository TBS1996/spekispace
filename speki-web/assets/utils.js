import * as git from "https://esm.sh/isomorphic-git@1.27.1";
import http from "https://esm.sh/isomorphic-git@1.27.1/http/web";

let fs;

const initBrowserFS = new Promise((resolve, reject) => {
    BrowserFS.configure({ fs: "IndexedDB", options: {} }, (err) => {
        if (err) {
            console.error("Failed to initialize BrowserFS:", err);
            reject(err);
        } else {
            fs = BrowserFS.BFSRequire("fs"); 
            console.log("BrowserFS initialized");
            resolve();
        }
    });
});

export async function deleteFile(path) {
    await initBrowserFS;

    return new Promise((resolve, reject) => {
        fs.unlink(path, (err) => {
            if (err) {
                console.error("Error deleting file:", err);
                reject("Error deleting file: " + err);
            } else {
                resolve("File deleted successfully");
            }
        });
    });
}


export async function saveFile(path, content) {
    await initBrowserFS;

    return new Promise((resolve, reject) => {
        fs.writeFile(path, content, "utf8", (err) => {
            if (err) {
                console.error("Error writing file:", err);
                reject("Error writing file: " + err);
            } else {
                resolve("File written successfully");
            }
        });
    });
}



export async function loadAllFiles(dirPath) {
    await initBrowserFS;
    console.log(dirPath);

    return new Promise((resolve, reject) => {
        fs.readdir( dirPath, (err, files) => {
            if (err) {
                console.error("Error reading directory:", err);
                reject("Error reading directory: " + err);
                return;
            }

            const fileContents = [];
            let filesRead = 0;

            console.log("sup");
            if (files.length === 0) resolve(fileContents); // Return empty array if no files

            files.forEach(file => {
                const filePath = `${dirPath}/${file}`;
                fs.readFile(filePath, "utf8", (err, data) => {
                    if (err) {
                        console.error(`Error reading file ${filePath}:`, err);
                        reject("Error reading file: " + err);
                        return;
                    }

                    fileContents.push(data);
                    filesRead++;


                    if (filesRead === files.length) {
                        resolve(fileContents); // Return contents of all files
                    }
                });
            });
        });
    });
}




export async function loadFile(path) {
    await initBrowserFS;
   // console.log(path);

    return new Promise((resolve, reject) => {
        fs.readFile(path, "utf8", (err, data) => {
            if (err) {
                if (err.code === "ENOENT") {
                    // If file does not exist, return null
                    console.warn(`File not found: ${path}`);
                    resolve(null);
                } else {
                    console.error("Error reading file:", err);
                    reject("Error reading file: " + err);
                }
            } else {
                resolve(data); // Return file contents as a string
            }
        });
    });
}





export async function listFiles(path) {
    const output = document.getElementById("output");

    await initBrowserFS;

    console.log(`Listing files from ${path}!`);

    try {
        // List files in the specified directory
        fs.readdir(path, (err, files) => {
            if (err) {
                output.textContent = "Error reading directory: " + err;
                return;
            }

            output.textContent += `\nFiles in ${path}:\n` + files.join("\n");
        });
    } catch (error) {
        output.textContent = "Failed to read repository: " + error;
    }
}

export async function cloneRepo(path, url, token) {
    const output = document.getElementById("output");

    await initBrowserFS;

    try {
    console.log(`starting clone from ${url} fles to ${path}!`);
        await git.clone({
            fs,
            http,
            dir: path, 
            url: url, 
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
}
