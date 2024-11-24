!#/bin/bash

VERSION=$(git describe)

echo "Building version agjini/bar:$VERSION"

docker build . -t agjini/bar:$VERSION

docker push agjini/bar:$VERSION
