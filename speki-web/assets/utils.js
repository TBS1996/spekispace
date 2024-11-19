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
    let s = await loadFile("/foobar/.git/config");
    console.log(s);

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


export async function fetchRepo(path, url, token, proxy) {
    const output = document.getElementById("output");

    await initBrowserFS;


    try {
        console.log(`starting fetch from ${url} files to ${path} with token ${token} through proxy ${proxy}!`);
	let cache = {};
        await git.fetch({
            fs,
            http,
	    cache,
            dir: path, 
            url: url, 
            corsProxy: proxy,
            singleBranch: true,
            onProgress: (progress) => {
                console.log(`${progress.phase}: ${progress.loaded} of ${progress.total}`);
              },
            depth: 1,
            onAuth: () => ({
                username: 'x-access-token',
                password: token
            })
        });
        console.log(`successsddd nice`);
        output.textContent = "Repository fetched successfully!";
    } catch (error) {
        output.textContent = "Failed to fetch repository: " + error;
    }
}

export async function cloneRepo(path, url, token, proxy) {
    const output = document.getElementById("output");

    await initBrowserFS;


    try {
        console.log(`starting clone from ${url} files to ${path} with token ${token} through proxy ${proxy}!`);
	let cache = {};
        await git.clone({
            fs,
            http,
	    cache,
            dir: path, 
            url: url, 
            corsProxy: proxy,
            onProgress: (progress) => {
                console.log(`${progress.phase}: ${progress.loaded} of ${progress.total}`);
              },
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


export async function pullRepo(repoPath, token, proxy) {
    const output = document.getElementById("output");

    await initBrowserFS;

    try {
        console.log(`Pulling latest changes in the repository at '${repoPath} with token ${token}'...`);
        const { name, email } = await fetchGitHubUserDetails(token);
        let cache = {};

        await git.pull({
            fs,
            http,
            cache,
            dir: repoPath,
            corsProxy: proxy,
            singleBranch: true,
            ref: 'main',
            depth: 1,
            onProgress: (progress) => {
                console.log(`${progress.phase}: ${progress.loaded} of ${progress.total}`);
              },
            onAuth: () => ({
                username: 'x-access-token',
                password: token
            }),
            author: {
                name,
                email
            }
        });

        const resultMessage = "Repository successfully updated with latest changes!";
        output.textContent = resultMessage;

        console.log(resultMessage);
        return resultMessage;
    } catch (error) {
        const errorMsg = `Failed to pull repository: ${error}`;
        output.textContent = errorMsg;
        console.error(errorMsg);
        throw new Error(errorMsg);
    }
}



// A utility function to check the git status of a repository
export async function gitStatus(path) {
    const output = document.getElementById("output");

    await initBrowserFS;

    try {
        console.log(`Checking git status for repo at ${path}...`);
        
        // Use the isomorphic-git `statusMatrix` function
        const statusMatrix = await git.statusMatrix({
            fs,
            dir: path
        });

        let statusSummary = "";

        // Analyze the status matrix
        for (const [filepath, head, workdir, stage] of statusMatrix) {
            if (head !== workdir || workdir !== stage) {
                statusSummary += `${filepath}: `;
                if (head === 0 && workdir !== 0) {
                    statusSummary += "Untracked\n";
                } else if (head !== workdir && workdir === stage) {
                    statusSummary += "Modified\n";
                } else if (workdir !== stage) {
                    statusSummary += "Staged\n";
                }
            }
        }

        if (!statusSummary) {
            statusSummary = "Working directory clean!";
        }

        output.textContent = statusSummary;
        return statusSummary;
    } catch (error) {
        const errorMsg = `Failed to get git status: ${error}`;
        output.textContent = errorMsg;
        throw new Error(errorMsg);
    }
}

export async function newReviews(repoPath) {
    const output = document.getElementById("output");

    await initBrowserFS;

        try {
        console.log(`Counting changed files in the 'reviews' folder within repo at '${repoPath}'...`);

        // Use isomorphic-git's `statusMatrix` to get file statuses
        const statusMatrix = await git.statusMatrix({
            fs,
            dir: repoPath
        });

        console.log(`supp`);

        // Filter files in the 'reviews' folder and count those with changes
        const changedFilesCount = statusMatrix.filter(([filepath, head, workdir, stage]) =>
            filepath.startsWith('reviews/') && (head !== workdir || workdir !== stage)
        ).length;

        console.log(`hey`);

        const resultMessage = `Number of changed files in the 'reviews' folder: ${changedFilesCount}`;
        output.textContent = resultMessage;

        console.log(resultMessage);
        return changedFilesCount;
    } catch (error) {
        const errorMsg = `Failed to count changed files in the 'reviews' folder: ${error}`;
        output.textContent = errorMsg;
        console.error(errorMsg);
        throw new Error(errorMsg);
    }
}

function gitCreds(){
    return {
        fs,
        http,
        dir: repoPath,
        corsProxy: "https://cors.isomorphic-git.org",
        singleBranch: true,
        onAuth: () => ({
            username: 'x-access-token',
            password: token
        }),
        author: {
            name: "myself",
            email: "myself@mymail.com"
        }
    }

}


async function fetchGitHubUserDetails(token) {
    return {
        name: "unknown user",
        email: "unknown@example.com"
    }

    const response = await fetch("https://api.github.com/user", {
        headers: {
            Authorization: `Bearer ${token}`,
            Accept: "application/vnd.github.v3+json"
        }
    });

    if (!response.ok) {
        throw new Error(`Failed to fetch user details: ${response.statusText}`);
    }

    const userData = await response.json();

    return {
        name: userData.name || "Unknown User",
        email: userData.email || "unknown@example.com" 
    };
}



export async function validateUpstream(repoPath, token) {
    try {
        await initBrowserFS;

        console.log(`Validating upstream connection for repository at '${repoPath}'...`);

        const customHttpClient = {
            request: async (url, options) => {
                // Add the "Origin" header for the CORS proxy
                options.headers = {
                    ...options.headers,
                    "Origin": "http://localhost:8080" // Replace with your app's actual origin
                };

                console.log("HTTP Request:", url, options);

                const response = await fetch(url, options);
                console.log("HTTP Response:", response.status, response.statusText);

                const body = await response.text();
                console.log("HTTP Response Body:", body);

                return new Response(body, {
                    status: response.status,
                    statusText: response.statusText,
                    headers: response.headers
                });
            }
        };


        await git.fetch({
            fs,
            http,
            dir: repoPath,
            //corsProxy: "http://localhost:8081",
            onAuth: () => ({
                username: 'x-access-token',
                password: token
            }),
            depth: 1, 
        });

        console.log("Upstream connection validated successfully!");
        return true;
    } catch (error) {
        console.error(`Failed to validate upstream connection: ${error}`);
        return false;
    }
}


export async function gitClone(dir, url, token, proxy) {
  console.log(`Initializing repository at ${dir}...`);
  await initBrowserFS;
  await git.init({ fs, dir });

  let ref = 'main';

  console.log(`Adding remote ${url} as "origin"...`);
  await git.addRemote({ fs, dir, remote: 'origin', url });

  await git.fetch({
    fs,
    http,
    corsProxy: proxy,
    dir,
    remote: 'origin',
    singleBranch: true,
    depth: 1,
    ref,
  });

const refs = await git.listBranches({ fs, dir, remote: 'origin' });
console.log('Fetched refs:', refs);

const commitOid = await git.resolveRef({ fs, dir, ref: 'refs/remotes/origin/main' });
console.log(`Commit OID for origin/main: ${commitOid}`);

const branches = await git.listBranches({ fs, dir });
console.log('Local branches:', branches);



if (!branches.includes(ref)) {
  console.log(`Creating and checking out local branch ${ref}...`);
  console.log(`Creating local branch ${ref}...`);
  await git.writeRef({
    fs,
    dir,
    ref: `refs/heads/${ref}`,
    value: commitOid, 
    force: true, 
  });

  // Update HEAD to point to the new branch
  await git.writeRef({
    fs,
    dir,
    ref: 'HEAD',
    value: `refs/heads/${ref}`,
    force: true,
  });
} else {
  console.log(`Branch ${ref} already exists locally. Checking it out...`);
  console.log(`Checking out branch ${ref}...`);
  await git.checkout({ fs, dir, ref });
}

const bs = await git.listBranches({ fs, dir });
console.log('Local branches:', bs);

const head = await git.resolveRef({ fs, dir, ref: 'HEAD' });
console.log('Current HEAD:', head);



  console.log(`Repository cloned into ${dir}`);
}