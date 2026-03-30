#!/bin/sh
cd "/home/diver/sources/JS/NSQCuT/"
j=$(date)
git add .
git commit -m "$1 $j"
git push git@github.com:Vladgobelen/NSQCuT.git
git add .
git commit -m "$1 $j"
git push

