#!/bin/bash

# Example:
# fakeroot ./make_varf.sh 1.2-2
set -e

VERSION=$1
BASE=varf_$VERSION
mkdir -p $BASE/DEBIAN

if [ -e varf ]
then
	cd varf
	git fetch && git rebase
else
	git clone https://github.com/Gyscos/varf
	cd varf
fi

VARF_HOME=/usr/share/varf cargo build --release

DESTDIR=../$BASE bash install.sh
cd ..

echo "Package: varf
Version: $VERSION
Section: base
Priority: optional
Architecture: amd64
Depends: libssl-dev (>=1.0)
Maintainer: Alexandre Bury <alexandre.bury@gmail.com>
Installed-Size: `du -s $BASE/usr | cut -f 1`
Description: Varf
 A small Arff web viewer" >> $BASE/DEBIAN/control

dpkg-deb --build $BASE
