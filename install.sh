#!/bin/sh
mkdir -p "$DESTDIR/usr/bin"
mkdir -p "$DESTDIR/usr/share/varf"

cp target/release/varf "$DESTDIR/usr/bin/"
cp -a data/* "$DESTDIR/usr/share/varf/"
