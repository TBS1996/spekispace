
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





export async function lastModified(filePath) {
  await initBrowserFS;

  return new Promise((resolve, reject) => {
      fs.stat(filePath, (err, stats) => {
        if (err) {
            return resolve(null);
        }

        resolve(stats.mtime);
      });
    });
}

export async function loadFilenames(directory) {
    await initBrowserFS;
    console.log(directory);
    return new Promise((resolve, reject) => {
        fs.readdir(directory, (err, files) => {
            if (err) reject(err);
            else resolve(files);
        });
    });

}
