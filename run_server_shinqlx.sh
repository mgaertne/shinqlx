#!/bin/bash
MACHINE_TYPE=`uname -m`
if [ ${MACHINE_TYPE} == 'x86_64' ]; then
    qlds_executable="qzeroded.x64"
else 
    qlds_executable="qzeroded.x86"
fi

basepath="$(dirname "$0")"
cd $basepath

export LD_PRELOAD=$LD_PRELOAD:$basepath/libshinqlx.so

LD_LIBRARY_PATH="$basepath/linux64:$LD_LIBRARY_PATH" exec $basepath/$qlds_executable "$@"
