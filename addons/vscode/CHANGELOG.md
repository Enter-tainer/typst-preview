# Change Log

All notable changes to the "typst-preview" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.


## 0.1.0

Initial release 

## 0.1.6

Add preview button

## 0.1.7

- Preview on type
- Add config entry for `typst-ws` path

## 0.2.1

- Bundle typst-ws within vsix. You no longer need to install typst-ws

## 0.2.2

- Fix server process not killed on exit(maybe)
- Add config for OnSave/OnType
- Add output channel for logging

## 0.2.3

- Performance Improvement: only update pages when they are visible. This should improve performance when you have a lot of pages.

## 0.2.4

- Automatically choose a free port to listen. This should fix the problem where you can't preview multiple files at the same time.
- Server will exit right after client disconnects, preventing resource leak.

## 0.3.0

- Upgrade typst to v0.3.0
- Fix panic when pages are removed

## 0.3.1

- Publish to OpenVSX
- allow configuring font paths

## 0.3.3

- Fix nix-ld compatibility by inheriting env vars(#33)

## 0.4.0

- Upgrade to typst v0.4.0

## 0.4.1

- Makes the WebSocket connection retry itself when it is closed, with a delay of 1 second.

## v0.5.0

- Upgrade to typst v0.5.0
