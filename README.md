# Mode Jeu

Application Windows légère (Tauri 2 + React) qui ferme les applications gourmandes avant une session de jeu, puis remet tout en place.

## Ce qu'elle fait

- Regroupe les processus par application avec leur consommation mémoire.
- Ferme tout ce qui n'est ni système, ni gardé par toi, ni au premier plan.
- Mémorise le chemin de chaque application fermée pour la relancer d'un clic.
- Bascule sur le plan d'alimentation Performances élevées et restaure le précédent à la sortie.
- Arrête les services optionnels (SysMain, WSearch) si l'app est lancée en administrateur.

## Sécurités

Une liste noire codée en dur protège les processus critiques (`lsass`, `csrss`, `dwm`, `explorer`, pilotes AMD/NVIDIA…). Les launchers de jeux (Steam, Epic, EA, Battle.net, GOG, Ubisoft, Riot, Xbox) et leurs processus associés (helpers, overlays, anti-cheats) sont aussi protégés par défaut, même sans être dans ta liste « gardées ». En plus, seuls les exécutables situés hors de `C:\Windows` sont candidats à la fermeture. Ces process protégés n'apparaissent même pas dans la liste.

## Démarrer

```bash
npm install
npm run app        # développement
npm run release    # installeur NSIS dans src-tauri/target/release/bundle
```

Prérequis : Rust stable, Node 18+, Visual Studio Build Tools (C++), WebView2 (déjà présent sur Windows 10/11 à jour).

## Fichiers de configuration

`%APPDATA%\com.pdcdesign.modejeu\config.json` et `session.json`.

```json
{
  "keep": ["steam.exe", "epicgameslauncher.exe", "discord.exe"],
  "high_performance": true,
  "protect_foreground": true,
  "stop_services": false,
  "services": ["SysMain", "WSearch"]
}
```

## Ajouter un composant shadcn

Le projet suit la structure shadcn (`src/components/ui`, alias `@/`, `cn()` dans `src/lib/utils.ts`). Colle simplement le code d'un composant depuis ui.shadcn.com, en remplaçant les tokens par ceux de `tailwind.config.js` (`ink`, `surface`, `raised`, `line`, `muted`, `paper`, `brass`, `jade`).
