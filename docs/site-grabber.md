# Site grabber

[← Back to docs index](README.md)

**Site grabber** explores a web site from a page you give it, follows its links a few levels deep, and lists the files it finds: every picture of a gallery, the PDFs of a documentation site, the recordings of a podcast's pages, or the whole site to read offline. You tick the ones you want and DDM downloads them like any other download, as one batch.

## Grab a site

1. Click **Grabber** on the toolbar, or choose **Tasks → Run site grabber...**.
2. Pick a **Template**: **All images**, **All audio**, **All video**, **All documents**, **Whole site for offline reading**, or **Custom**. A template sets which files are collected and how deep the grabber goes; you can change both later.
3. Type the **Start address**, the page to start from, for example `https://example.com/gallery/`. Without `http://` or `https://`, DDM uses `https://`.
4. If the site asks for a user name and password, type them under **Login and cookies**. A site that needs you signed in with its own page takes the **Cookies** your browser sends it, written as the browser sends them: `name=value; name2=value2`. The login and the cookies are sent to the start address's site only, never to another site its pages link to. A login saved in **Options → Sites Logins** for that site is used too.
5. Click **Next**, choose where the files go (see below), click **Next** again, check what to explore, and click **Start**.

While it works, the window shows how many pages it has explored, how many are waiting, how many files it has found, and the address it is reading. **Stop** ends it at once; what it found so far is kept.

You can close the window while it explores: exploring goes on. The project shows under **Grabber projects** in the Categories pane, marked "(exploring)"; open it there, or with **Tasks → Grabber projects**, to see how far it is. When DDM closes, a grab under way stops and keeps what it found, to continue at the next start.

## Where the files go

- **Save to**: a folder of your choice. Left empty, each file goes to its category's folder (**Options → Save to**), as other downloads do.
- **Keep the site's folders**: under the folder, each file goes where it is on the site: `example.com\photos\2025\harbour.jpg`. **Put every file in this one folder** puts them side by side instead. A name that is taken gets a number, as always.
- **Category**: the category the downloads are listed under, or **Each by its file type**.
- **Make the pages work offline**: see [Read a site offline](#read-a-site-offline).
- **List the downloads under All Downloads too**: off by default, so that a grab of thousands of files doesn't bury your other downloads. The files show under the project in **Grabber projects**, and in the queue they join. Tick it to see them under All Downloads and its categories as well.

DDM makes the folder when the first file goes there; it never asks about it.

## What it explores and collects

- **Depth**: how many links away from the start page it reads pages. 0 reads the start page only; 2 reads the pages it links to, and the pages those link to.
- **Explore pages on**: **The same host** (the default; `www.` or not), **The same domain and its subdomains** (`example.com`, `img.example.com`), or **Any site**.
- **Only pages under the start address's folder**: part of a site. From `https://example.com/docs/intro.html`, only the pages in `/docs/` and below it are explored, not the rest of the site. The pictures and files those pages use are collected wherever they are.
- **Collect files from other sites too**: pictures and files the pages link to on another site (a site's images on a separate image server) are collected too. Untick it to keep to the sites explored.
- **File types**: Images, Video, Audio, Documents, Archives, Programs, **Web pages** (the pages themselves) and **Styles, scripts, fonts** (what pages need to look right). **Other types** takes extensions of your own, separated by spaces: `PSD RAW`.
- **Only files whose address is like** and **Leave out addresses like**: patterns, separated by spaces. `*` stands for any text and `?` for one character; a pattern without them matches any address that contains it. `*/photos/*` collects only files in a `photos` folder; `thumb` leaves out every address with `thumb` in it. Addresses left out are neither explored nor collected. Letter case doesn't count.
- **Most pages** and **Most files**: the grabber stops when it has explored or collected that many, and says so. **Continue** then goes on from where it stopped, with as many again.
- **Pause between requests** and **Requests at once**: how gently it reads the site. The defaults (half a second, two at once) are polite to small sites. Each page is read once, however many pages link to it.
- **Follow the site's robots.txt** (on by default): pages the site asks robots not to visit are not explored, and a `Crawl-delay` it asks for is kept (up to 10 seconds). The window counts the addresses it left out for it.
- **Look up each file's size**: once exploring is done, DDM asks the server each file's size. Untick it for a faster list. **Whole site for offline reading** leaves it off: the downloads say each size anyway.

The grabber finds links where a browser does: links, frames, pictures (`srcset`, `<picture>` and pictures loaded late from `data-src` too), videos and their posters, sounds, style sheets and the pictures and fonts in them, scripts, icons. Relative addresses, addresses from the site's root, `//host/…` addresses, a `<base href>`, addresses with letters beyond ASCII and redirects are followed as a browser follows them. It doesn't run the pages' scripts, so files that only a script adds to a page aren't found. A link whose address doesn't show what it is (`/download?id=7`) is asked once: a page is explored, a file is collected under the name the server gives it. A page that doesn't answer is counted under **Could not be read**; **Continue** asks it again. Cookies the site sets while it is explored (a session, a consent) are sent back to it for the rest of the grab, and with the downloads of its files.

## Read a site offline

**Whole site for offline reading** (or **Make the pages work offline** on any project) saves a site so that it opens and works from the disk, without a connection:

- Each file is saved where its address says, under the folder: `example.com\docs\guide.html`. A page always ends in `.html` so a browser opens it (`/about` becomes `about.html`, a folder's page `index.html`, `/post?id=2` becomes `post@id=2.html`).
- As each page and style sheet finishes downloading, DDM changes its links to lead to the saved copies: links to other pages, pictures (`srcset` and pictures loaded late too), style sheets, scripts, fonts and the addresses in CSS. A link to something that wasn't saved (a page beyond the depth, another site) is made a full address, so it still opens on the site. Nothing else in the page changes. Grabbing more of the site later and downloading the new files updates the pages already saved.
- A page named after the project, at the top of the folder (`example.com.html`), opens the copy's start page. You can also right-click the project and choose **Open offline copy** once its start page is downloaded.

What a copy can't do: run what needs the site (searches, forms, logins), or show what a page's scripts would have fetched. Some browsers refuse scripts written as modules (`type=module`) from the disk.

## Download what it found

When it ends, the window lists the files: their name, address, type and size. They are all ticked, except those already in your download list.

- Type in **Filter** to show only the files whose name or address contains it, and choose a **Type** to show only the pictures, the documents, the pages and so on. **Larger than** and **Smaller than** (in KB) show only the files of those sizes, to leave out thumbnails say; a file whose size isn't known is always shown. **Select all** and **Select none** tick or untick the files shown.
- Click a column's title to sort by it; click it again for the other way.
- **Add to queue** chooses the queue they join, and **Start the queue now** whether it starts. The queue downloads as many at once as it allows.
- **Download selected** adds the ticked files to the download list; **Download all** adds every file found. They are added as one batch: no window opens for any of them, as it is added or as it finishes. Each download carries the page it was found on as its referrer, as the browser would.
- **Continue** (when the last run stopped early) explores further, from where it stopped.

The main window then shows the project's node, with its downloads.

## Grabber projects

Each grab is kept as a project, listed under **Grabber projects** in the Categories pane and, the latest first, under **Tasks → Grabber projects**. Click a project to see the downloads it added; while it has none, the list offers **Open the project**. Right-click it for:

- **Open**: its window, on its grab under way, else on the files it found (or its settings, when it hasn't run yet).
- **Continue exploring**: go on from where its last run stopped (Stop, Most pages, Most files, DDM closed).
- **Run again**: explore the site again with the same settings (new files on the site are found; files already in the list come unticked).
- **Edit...**: open its settings in Site grabber.
- **Open offline copy**: its saved start page, in your browser (a project whose pages work offline).
- **Delete**: forget the project. Its downloads stay in the list (under All Downloads again) and its files on the disk.

Projects are saved in the data folder (`grabber.json`), with the files their last run found and where it stopped. A project's password and cookies are saved as the other passwords in the settings are: sealed for your Windows account, and readable only by you on Linux and macOS.

## Next steps

- [Link grabber and archives →](link-grabber.md)
- [Queues and the Scheduler →](queues-and-scheduler.md)
- [Options → Sites Logins →](options.md)
