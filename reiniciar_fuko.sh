#!/usr/bin/env bash
# ==============================================================================
# reiniciar_fuko.sh - Detener e iniciar Fuko Desktop con la última versión
# ==============================================================================
set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
PURPLE='\033[0;35m'
NC='\033[0m'

echo -e "${PURPLE}======================================================${NC}"
echo -e "${PURPLE}       🐱 REINICIANDO FUKO DESKTOP                   ${NC}"
echo -e "${PURPLE}======================================================${NC}"

echo -e "${BLUE}[1/3] Cerrando instancias activas de Fuko...${NC}"
pkill -f "claudia-desktop" 2>/dev/null || true
pkill -f "claudia_ears_daemon.py" 2>/dev/null || true
sleep 1

echo -e "${BLUE}[2/3] Verificando ejecutable...${NC}"
FUKO_DIR="$HOME/fuko-desktop"
LAUNCHER="$FUKO_DIR/start-fuko.sh"

if [ ! -f "$LAUNCHER" ]; then
    echo -e "${YELLOW}No se encontró start-fuko.sh en $FUKO_DIR.${NC}"
    exit 1
fi

chmod +x "$LAUNCHER"

echo -e "${BLUE}[3/3] Iniciando Fuko en segundo plano...${NC}"
cd "$FUKO_DIR"
nohup setsid "$LAUNCHER" > "$FUKO_DIR/fuko.log" 2>&1 < /dev/null &
PID=$!

sleep 1.5
if ps -p $PID > /dev/null; then
    echo -e "${GREEN}✓ Fuko está corriendo activamente (PID: $PID).${NC}"
    echo -e "${GREEN}✨ ¡Actualización aplicada correctamente en tu escritorio!${NC}"
else
    echo -e "${YELLOW}Iniciado en segundo plano. Registros en: $FUKO_DIR/fuko.log${NC}"
fi

echo -e "${PURPLE}======================================================${NC}"
