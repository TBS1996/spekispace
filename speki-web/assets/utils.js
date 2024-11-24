import * as git from "https://esm.sh/isomorphic-git@1.27.1";
import http from "https://esm.sh/isomorphic-git@1.27.1/http/web";
import * as path from "https://esm.sh/path-browserify";

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
                console.log("loading..");
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


export async function allPaths(repoPath) {
    const subdirs = ["cards", "attributes", "reviews"];
    const allFilePaths = [];

    for (const subdir of subdirs) {
        const fullSubdirPath = `${repoPath}/${subdir}`;
        console.log(fullSubdirPath);
        try {
            const filePaths = await getFilePaths(fullSubdirPath); 
            console.log(filePaths);

            const relativePaths = filePaths.map((filePath) =>
                filePath.replace(`${repoPath}/`, '') 
            );

            console.log(relativePaths);

            allFilePaths.push(...relativePaths);
        } catch (err) {
            console.error(`Failed to get files from ${fullSubdirPath}: ${err}`);
        }
    }

    return allFilePaths;
}




async function getFilePaths(folderPath) {
    await initBrowserFS;

    return new Promise((resolve, reject) => {
        fs.readdir(folderPath, (err, files) => {
            if (err) {
                reject(`Error reading directory: ${err}`);
                return;
            }

            // Prepend the folder path to each file name to get the full path
            const fullPaths = files.map((file) => `${folderPath}/${file}`);
            resolve(fullPaths);
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


export async function fetchRepo(path, token, proxy) {
    const output = document.getElementById("output");

    await initBrowserFS;
        console.log(`starting fetch from files to ${path} with token ${token} through proxy ${proxy}!`);
        await git.fetch({
            fs,
            http,
            dir: path, 
            corsProxy: proxy,
            singleBranch: true,
            onProgress: (progress) => {
                console.log(`${progress.phase}: ${progress.loaded} of ${progress.total}`);
              },
            onAuth: () => ({
                username: 'x-access-token',
                password: token
            })
        });
        console.log(`successsddd nice`);
        output.textContent = "Repository fetched successfully!";
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

//////////////

async function commit(repoPath, token){
    await initBrowserFS;
        const { name, email } = await fetchGitHubUserDetails(token);
        await git.commit({
            fs,
            dir: repoPath,
            message: "commit",
            author: {
                name,
                email
            }
        });
}


async function modifiedFiles(repopath) {
    await initBrowserFS;
    const FILE = 0, HEAD = 1, WORKDIR = 2

    const filenames = (await git.statusMatrix({ fs,dir: repopath }))
      .filter(row => row[HEAD] !== row[WORKDIR])
      .map(row => row[FILE]);

      console.log(filenames);
      return filenames;
}


async function addAllFiles(repopath) {
    await initBrowserFS;

    let paths = await modifiedFiles(repopath);
    console.log(paths);

    console.log("Adding all files...");
    await Promise.all(
        paths.map((path) => addFile(repopath, path))
    );
    console.log("All files added!");
}

async function addFile(repoPath, filepath) {
    console.log(`the file to add: ${filepath} `);
    await git.add({ fs, dir: repoPath, filepath });
    console.log("adding file...");
}

export async function pushRepo(repoPath, token, proxy) {
    const output = document.getElementById("output");

    await initBrowserFS;

        console.log(`Pushing latest changes in the repository at '${repoPath} with token ${token}'...`);
        const { name, email } = await fetchGitHubUserDetails(token);
        let cache = {};

        await git.push({
            fs,
            http,
            cache,
            dir: repoPath,
            corsProxy: proxy,
            ref: 'main',
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
}

export async function syncRepo(repoPath, token, proxy) {
    console.log("adding files..");
    await addAllFiles(repoPath);
    console.log("commiting files..");
    await commit(repoPath, token);
    console.log("pulling repo..");
    await pullRepo(repoPath, token, proxy);
    console.log("pushing repo..");
    await pushRepo(repoPath, token, proxy);
}


async function mergeRepo(repoPath, token) {
    await initBrowserFS;

    console.log(`merging repository at '${repoPath}...`);
    const { name, email } = await fetchGitHubUserDetails(token);

    const mergeDriver = async ({ contents, path }) => {
        const baseContent = contents[0];
        const ourContent = contents[1];
        const theirContent = contents[2];

        console.log(`merging path: ${path}`);


        const pattern = /^\d{10} \d/;
        let isReview =  pattern.test(baseContent) || pattern.test(ourContent) || pattern.test(theirContent);
      
        if (!isReview) {
            console.log(`choosing upstream commit for file: ${theirContent} `);
          return { cleanMerge: true, mergedText: theirContent };
        }
      
        const combinedLines = [
          ...baseContent.split('\n'),
          ...ourContent.split('\n'),
          ...theirContent.split('\n'),
        ];
        console.log(combinedLines);
        const uniqueSortedLines = [...new Set(combinedLines)].sort();
        console.log(uniqueSortedLines);

        let mergedText = uniqueSortedLines.join('\n').trim();
        console.log(mergedText);
      
        return {
          cleanMerge: true,
          mergedText,
        };
      };

    await git.merge({
        fs,
        ours: 'main',
        theirs: 'remotes/origin/main',
        dir: repoPath,
        mergeDriver,
        author: {
            name,
            email
        }
    });

    console.log("Repository successfully merged with latest changes!");
}


export async function pullRepo(repoPath, token, proxy) {
    await fetchRepo(repoPath, token, proxy);
    await mergeRepo(repoPath, token);
}

///////////////////////////////////////////////////////////////




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



export async function loadRec(dirPath) {
  await initBrowserFS;

    const allFiles = [];
    
    await new Promise((resolve, reject) => {
        fs.readdir(dirPath, async (err, entries) => {
            if (err) {
                console.error("Error reading directory:", err);
                reject("Error reading directory: " + err);
                return;
            }

            let entriesProcessed = 0;

            if (entries.length === 0) resolve(allFiles); // Return if empty

            entries.forEach(async (entry) => {
                console.log(dirPath);
                console.log(entry);

                const entryPath = path.join(dirPath, entry);

                // Check if it's a directory or file
                fs.stat(entryPath, async (err, stats) => {
                    if (err) {
                        console.error(`Error reading file stats ${entryPath}:`, err);
                        reject("Error reading file stats: " + err);
                        return;
                    }

                    if (stats.isDirectory()) {
                        // Recursive call for directories
                        const subDirFiles = await loadRec(entryPath);
                        allFiles.push(...subDirFiles);
                    } else if (stats.isFile()) {
                        // Load the file contents
                        const fileContents = await loadAllFiles(dirPath);
                        allFiles.push(...fileContents);
                    }

                    entriesProcessed++;
                    if (entriesProcessed === entries.length) {
                        resolve(allFiles);
                    }
                });
            });
        });
    });

    return allFiles;
}

function readdir(dirPath) {
    return new Promise((resolve, reject) => {
        fs.readdir(dirPath, (err, files) => {
            if (err) reject(err);
            else resolve(files);
        });
    });
}

function stat(filePath) {
    return new Promise((resolve, reject) => {
        fs.stat(filePath, (err, stats) => {
            if (err) reject(err);
            else resolve(stats);
        });
    });
}

function unlink(filePath) {
    return new Promise((resolve, reject) => {
        fs.unlink(filePath, (err) => {
            if (err) reject(err);
            else resolve();
        });
    });
}

function rmdir(dirPath) {
    return new Promise((resolve, reject) => {
        fs.rmdir(dirPath, (err) => {
            if (err) reject(err);
            else resolve();
        });
    });
}

export async function deleteDir(dirPath) {
    await initBrowserFS;
    console.log("deleting dir: ${dirPath}");

    const entries = await readdir(dirPath);

    for (const entry of entries) {
        const fullPath = `${dirPath}/${entry}`;
        const stats = await stat(fullPath);

        if (stats.isDirectory()) {
            await deleteDir(fullPath);
        } else {
            await unlink(fullPath);
        }
    }

    await rmdir(dirPath);
    console.log("deleted!");
}