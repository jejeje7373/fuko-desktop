#!/usr/bin/env bash
# ==============================================================================
# actualizar_fuko.sh - Script de Sincronización y Actualización de Fuko Desktop
# ==============================================================================
set -e

# Colores para salida de terminal
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

echo -e "${PURPLE}======================================================${NC}"
echo -e "${PURPLE}       🐱 ACTUALIZADOR DE FUKO DESKTOP               ${NC}"
echo -e "${PURPLE}======================================================${NC}"

# Resolver directorio destino ($1 o por defecto ~/fuko-desktop)
TARGET_DIR="${1:-$HOME/fuko-desktop}"
# Expandir tilde si viene como literal ~/
TARGET_DIR="${TARGET_DIR/#\~/$HOME}"

WORKSPACE_DIR="$HOME/.openclaw/workspace/claudia-desktop"
OPENCLAW_WORKSPACE="$HOME/.openclaw/workspace"

echo -e "${BLUE}[1/5] Verificando rutas...${NC}"
echo -e "  - Workspace origen : ${WORKSPACE_DIR}"
echo -e "  - Repositorio GitHub: ${TARGET_DIR}"

if [ ! -d "$WORKSPACE_DIR" ]; then
    echo -e "${RED}[ERROR] No se encontró el directorio de trabajo: $WORKSPACE_DIR${NC}"
    exit 1
fi

if [ ! -d "$TARGET_DIR" ]; then
    echo -e "${YELLOW}[AVISO] El directorio destino no existe. Creándolo...${NC}"
    mkdir -p "$TARGET_DIR"
fi

echo -e "\n${BLUE}[2/5] Sincronizando archivos del proyecto a $TARGET_DIR...${NC}"
# Sincronizar archivos esenciales excluyendo temporales, node_modules y target/ de compilación
rsync -av --delete \
    --exclude=".git" \
    --exclude="node_modules" \
    --exclude="target" \
    --exclude="*.log" \
    --exclude="__pycache__" \
    --exclude=".DS_Store" \
    "$WORKSPACE_DIR/" "$TARGET_DIR/"

# Sincronizar SOUL.md e IDENTITY.md si están en el workspace raíz
if [ -f "$OPENCLAW_WORKSPACE/SOUL.md" ]; then
    cp -u "$OPENCLAW_WORKSPACE/SOUL.md" "$TARGET_DIR/SOUL.md"
    echo -e "  - SOUL.md sincronizado."
fi
if [ -f "$OPENCLAW_WORKSPACE/IDENTITY.md" ]; then
    cp -u "$OPENCLAW_WORKSPACE/IDENTITY.md" "$TARGET_DIR/IDENTITY.md"
    echo -e "  - IDENTITY.md sincronizado."
fi

# Copiar el script actual al destino y al workspace
cp -u "$0" "$TARGET_DIR/actualizar_fuko.sh" 2>/dev/null || true
cp -u "$0" "$WORKSPACE_DIR/actualizar_fuko.sh" 2>/dev/null || true
chmod +x "$TARGET_DIR/actualizar_fuko.sh" "$WORKSPACE_DIR/actualizar_fuko.sh" 2>/dev/null || true

# Asegurar start-fuko.sh
if [ -f "$TARGET_DIR/start-claudia.sh" ] && [ ! -f "$TARGET_DIR/start-fuko.sh" ]; then
    cp "$TARGET_DIR/start-claudia.sh" "$TARGET_DIR/start-fuko.sh"
    chmod +x "$TARGET_DIR/start-fuko.sh"
fi

echo -e "\n${BLUE}[3/5] Verificando estado de Git en $TARGET_DIR...${NC}"
cd "$TARGET_DIR"

if [ ! -d ".git" ]; then
    echo -e "${YELLOW}Iniciando repositorio Git...${NC}"
    git init
    git branch -M main
fi

# Comprobar si hay cambios
CHANGES=$(git status --porcelain)

if [ -z "$CHANGES" ]; then
    echo -e "${GREEN}✓ No hay cambios pendientes. El repositorio ya está al día.${NC}"
else
    echo -e "${YELLOW}Se detectaron los siguientes cambios:${NC}"
    git status -s

    echo -e "\n${BLUE}[4/5] Añadiendo cambios y creando commit...${NC}"
    git add -A

    COMMIT_MSG="${2:-feat: sincronización de Fuko con mejoras de Capri, cascada de modelos OpenClaw y Wayland perches}"
    git commit -m "$COMMIT_MSG"
    echo -e "${GREEN}✓ Commit creado: '$COMMIT_MSG'${NC}"

    echo -e "\n${BLUE}[5/5] Subiendo a GitHub...${NC}"
    if git remote get-url origin >/dev/null 2>&1; then
        git push origin main
        echo -e "${GREEN}✓ ¡Cambios subidos exitosamente a GitHub!${NC}"
    else
        echo -e "${YELLOW}[AVISO] No hay remote 'origin' configurado. Omite push.${NC}"
    fi
fi

echo -e "\n${PURPLE}======================================================${NC}"
echo -e "${GREEN}✨ ¡Fuko ha sido actualizado con éxito!${NC}"
echo -e "${PURPLE}======================================================${NC}"
