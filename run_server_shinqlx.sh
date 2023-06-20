#!/bin/bash
MACHINE_TYPE=`uname -m`
if [ ${MACHINE_TYPE} == 'x86_64' ]; then
    qlds_executable="qzeroded.x64"
else 
    qlds_executable="qzeroded.x86"
fi

pip_location=$(pip show shinqlx | grep Location | cut -f2 -d':')
if [ -z "${pip_location}" ]; then
    echo "shinqlx not found in current pip environment. Did you install it properly?"
    return 1
fi

shinqlx_lib=$(find ${pip_location}/shinqlx -name "*.so")
if [ -z "${pip_location}" ]; then
    echo "shinqlx library not found in current pip environment. Did you install it properly?"
    return 1
fi

basepath="$(dirname "$0")"
cd $basepath

export LD_PRELOAD=$LD_PRELOAD:${shinqlx_lib}

LD_LIBRARY_PATH="$basepath/linux64:$LD_LIBRARY_PATH" exec $basepath/$qlds_executable "$@"
