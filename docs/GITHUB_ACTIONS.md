# Building via GitHub Actions (no installs on your PC)

This repo includes `.github/workflows/build.yml`, which makes GitHub's
own cloud servers compile the Windows `.exe` and Android `.apk` for you
automatically. Nothing is installed on your computer for this path.

## 1. Create the repo

1. Go to https://github.com/new (you're signed in as `papayasds-ai`).
2. Repository name: `refresh` (or anything you like).
3. Choose **Private** (recommended — this is remote-access software
   for your own devices) or Public, your call.
4. **Do not** check "Add a README" — leave it empty.
5. Click **Create repository**.

## 2. Upload the project

On the new repo's page, click **"uploading an existing file"**.

Unzip `refresh-source.zip` on your computer first. Then, in your file
explorer, open the unzipped `refresh` folder so you're looking at its
*contents* (the `core`, `windows-host`, `android-client`, `.github`
folders etc. — not the outer `refresh` folder itself). Select all of
them and drag them into the GitHub upload page in your browser
(Chrome/Edge preserve the folder structure when you drag folders in).

Scroll down and click **Commit changes**.

## 3. Watch it build

Click the **Actions** tab at the top of the repo. You'll see a
workflow run start automatically (triggered by your upload). Click
into it — there are two jobs, `build-windows-host` and
`build-android`, running in parallel. Each takes roughly 5–15 minutes
the first time.

## 4. Download the files

Once a job shows a green checkmark, click on that finished run, then
scroll to the **Artifacts** section at the bottom of the page:

- **refresh-windows-host** → contains `refresh-windows-host.exe` and `pair.exe`
- **refresh-android** → contains `refresh.apk`

Click each to download a zip of that artifact. These are the real
compiled files — exactly what `build-all.bat` would have produced
locally, just built on GitHub's machines instead of yours.

## If a job fails (red X instead of green check)

Click into the failed job to see the exact error — the same kind of
compiler/build error you'd see locally, just readable in the browser.
Paste it back and it can be fixed the same way as a local build error;
nothing about *why* it failed is different just because it ran on
GitHub instead of your PC.

## Re-building after a fix

Any time you push a change (upload a file again, or `git push` if you
later install git), the workflow reruns automatically. You don't need
to re-trigger it manually.
