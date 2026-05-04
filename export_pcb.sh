#!/bin/sh

cd "$(dirname "$0")"

set -e

USE_LOCAL=false
if [ "$1" = "--local" ]; then
    USE_LOCAL=true
fi

OUT=target/kicad/dpedal_gerber_files
ZIP=dpedal_gerber_files.zip
PCB=pcb/dpedal_pcb.kicad_pcb

kicad_cli() {
    if [ "$USE_LOCAL" = true ]; then
        kicad-cli "$@"
    else
        docker run --rm \
            --user "$(id -u):$(id -g)" \
            -e HOME=/tmp \
            -v "$(pwd):/work" \
            -w /work \
            kicad/kicad:9.0 \
            kicad-cli "$@"
    fi
}

rm -f $ZIP

mkdir -p $OUT
kicad_cli pcb export drill --output $OUT $PCB
kicad_cli pcb export gerbers --output $OUT $PCB
cd target/kicad
zip $ZIP dpedal_gerber_files/*
mv $ZIP ../..
cd -

kicad_cli pcb export step --subst-models --output dpedal_pcb.step $PCB
