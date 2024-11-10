function helloWorld() {
    return "Hello, World!";
}

import * as git from "isomorphic-git";
import http from "isomorphic-git/http/web";

async function cloneRepo(url, dir) {
    await git.clone({
        fs,
        http,
        dir,
        url,
        singleBranch: true,
        depth: 1,
    });
}

//window.cloneRepo = cloneRepo;
