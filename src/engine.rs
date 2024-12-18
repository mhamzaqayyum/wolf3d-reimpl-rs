use core::panic;

use sdl2::{pixels::Color, rect::Rect};
use crate::{
    multimedia::{Multimedia, LightTexture, TextureType},
    inputs_buffer::InputsBuffer,
    player::Player,
    map::Map,
    utils::{
        ray::Ray,
        dda::RayCursor, vec2d::{Dot, Vec2, Point2, iPoint2}, conventions::TEXTURE_PITCH
    }, tiles::{Tile, TextureHandle, Sprite, WallSlice}
};
use crate::enemy::Enemy;
use crate::utils::vec2d::iVec2;

struct SpriteRenderData {
    vecToSprite: Vec2,
    spriteHitDistY: f64,
    spriteHitDistX: f64,
    spriteScreenX: i32,
    spriteRenderHeight: i32,
    spriteScreenRect: Rect,
    spriteTextureHandle: TextureHandle
}

pub struct GameEngine {
    pub multimedia: Multimedia,
    pub inputsBuffer: InputsBuffer,
    pub player: Player,
    pub map: Map,

    // Time related
    doorMoveIncr: f64,
    doorTimerIncr: f64,
    playerMoveIncr: f64,
    playerSwivelIncr: f64,

    // Render related
    wallSlicesBuffer: Vec<WallSlice>,
    spritesBuffer: Vec<Sprite>,
    spritesRenderDataBuffer: Vec<SpriteRenderData>,
    wallRenderHeights: Vec<i32>,
    spriteTileHitMap: Vec<Vec<bool>>,
    weaponRenderTopLeft: iVec2,
    weaponRenderPitch: i32,

    // Enemy related
    enemies: Vec<Enemy>
}

impl GameEngine {
    pub fn Init(windowWidth: usize, windowHeight: usize, fov: f64, mapCSVPath: &str) -> Self {
        let multimedia = Multimedia::New(windowWidth, windowHeight, fov);
        let inputsBuffer = InputsBuffer{windowLock: true, ..Default::default()};
        let player = Player::New(Point2::New(22.5, 2.5), multimedia.displayParams.refreshRate);
        let (map, enemies): (Map, Vec<Enemy>) = Map::LoadFromCSV(mapCSVPath, multimedia.displayParams.refreshRate);
        
        let refreshRatePropr = multimedia.displayParams.refreshRate as f64 / 60.0;
        let doorMoveIncr = 0.02/refreshRatePropr;
        let doorTimerIncr = 0.01/refreshRatePropr;
        let playerMoveIncr = 0.08/refreshRatePropr;
        let playerSwivelIncr = 0.00125/refreshRatePropr;

        let wallRenderHeights: Vec<i32> = vec![0; multimedia.windowParams.width];

        let spriteTileHitMap: Vec<Vec<bool>> = vec![vec![false; map.height as usize]; map.width as usize];

        let weaponRenderPitch = (windowWidth/2) as i32;
        let weaponRenderX = (windowWidth/2) - (weaponRenderPitch/2) as usize;
        let weaponRenderY = windowHeight - weaponRenderPitch as usize;

        Self {
            multimedia,
            inputsBuffer,
            player,
            map,

            doorMoveIncr,
            doorTimerIncr,
            playerMoveIncr,
            playerSwivelIncr,

            wallSlicesBuffer: Vec::new(),
            spritesBuffer: Vec::new(),
            spritesRenderDataBuffer: Vec::new(),
            wallRenderHeights,

            spriteTileHitMap,

            weaponRenderTopLeft: iVec2::New(weaponRenderX as i32, weaponRenderY as i32),
            weaponRenderPitch,

            enemies
        }
    }

    pub fn GameLoop(&mut self) {
        loop {
            self.Update();
            if self.inputsBuffer.quit { break; }
            self.RenderFrame();
        }
    }

    fn Update(&mut self) {
        self.inputsBuffer.Update(&mut self.multimedia.sdlContexts.sdlContext, &mut self.multimedia.sdlEventPump);
        self.UpdateEnemies();
        self.player.Update(&self.inputsBuffer, &mut self.map, &mut self.enemies, self.playerMoveIncr, self.playerSwivelIncr);
        self.map.UpdateDoors(self.doorMoveIncr, self.doorTimerIncr, self.player.location);
    }

    fn RenderFrame(&mut self) {
        self.multimedia.sdlCanvas.clear();
        self.DrawCeilingAndFloor();
        self.RenderIntoBuffers();
        self.DrawWallsFromBuffer();
        self.DrawSpritesFromBuffer();
        self.DrawWeapon();
        self.multimedia.sdlCanvas.present();
    }

    fn DrawCeilingAndFloor(&mut self) {
        self.multimedia.sdlCanvas.set_draw_color(Color::RGBA(50, 50, 50, 255));
        self.multimedia.sdlCanvas.fill_rect(Rect::new(0, 0, self.multimedia.windowParams.width as u32, (self.multimedia.windowParams.height/2) as u32)).unwrap();

        self.multimedia.sdlCanvas.set_draw_color(Color::RGBA(96, 96, 96, 255));
        self.multimedia.sdlCanvas.fill_rect(Rect::new(0, (self.multimedia.windowParams.height / 2) as i32, self.multimedia.windowParams.width as u32, (self.multimedia.windowParams.height/2) as u32)).unwrap();
    }

    fn RenderIntoBuffers(&mut self) {
        self.wallSlicesBuffer.clear();
        self.spritesBuffer.clear();
        self.ResetSpriteTileHitMap();

        for x in 0..self.multimedia.windowParams.width {
            let currRay = Ray::New(self.player.location, self.player.viewDir.Rotate(self.multimedia.renderParams.castingRayAngles[x].0));
            let mut rayCursor = RayCursor::New(currRay, self.player.location);
            let mut prevTileCoord = rayCursor.hitTile;
            while self.map.WithinMap(rayCursor.hitTile) {
                let prevTileWasDoor = if let Tile::DOOR(_) = self.map.GetTile(prevTileCoord) { true } else { false };
                rayCursor.GoToNextHit();
                let currTileCoord = rayCursor.hitTile;
                prevTileCoord = currTileCoord;                

                match self.map.GetTile(currTileCoord) {
                    Tile::WALL(wall) => {
                        let mut wallSlice = wall.GetWallSlice(&mut rayCursor);
                        if prevTileWasDoor {
                            let gateSidewall_lit = TextureHandle::New(TextureType::WALL, 101);
                            let gateSideWall_unlit = TextureHandle::New(TextureType::WALL, 102);
                            wallSlice.textureHandle = LightTexture(&mut rayCursor, gateSidewall_lit, gateSideWall_unlit);
                        }
                        self.wallSlicesBuffer.push(wallSlice);
                        break;
                    },
                    Tile::DOOR(door) => {
                        let doorWallSlice = door.GetWallSlice(&mut rayCursor);
                        if doorWallSlice.is_some() {
                            self.wallSlicesBuffer.push(doorWallSlice.unwrap());
                            break;
                        } else {
                            continue;
                        }
                    },
                    Tile::OBJECT(_) | Tile::EMPTY(_) => {
                        self.GrabSprites(currTileCoord)
                    },
                    Tile::NONE => panic!(),
                };
            }
        }
    }

    fn DrawWallsFromBuffer(&mut self) {
        self.ResetWallRenderHeights();
        for x in 0..self.wallSlicesBuffer.len() {
            let wallSlice = &self.wallSlicesBuffer[x];

            let distToHitPoint = wallSlice.dist;
            let renderHeight = (self.multimedia.renderParams.renderHeightProprConst / (distToHitPoint * self.multimedia.renderParams.castingRayAngles[x as usize].1)) as i32;
            let screenY = (self.multimedia.windowParams.height/2) as i32 - (renderHeight / 2);
            let screenRect = Rect::new(x as i32, screenY, 1, renderHeight as u32);
            self.wallRenderHeights[x as usize] = renderHeight;

            let texture = self.multimedia.assets.GetTexture(wallSlice.textureHandle);

            // Render onto screen
            let _ = self.multimedia.sdlCanvas.copy(texture, wallSlice.textureRect, screenRect);
        }
    }

    fn DrawSpritesFromBuffer(&mut self) {
        self.spritesRenderDataBuffer.clear();
        for sprite in &self.spritesBuffer {            
            let vecToSprite = sprite.location - self.player.location;
            let spriteHitDistY = Dot(vecToSprite, self.player.viewDir);
            let spriteHitDistX = Dot(vecToSprite, self.player.east);
            let spriteScreenX = ((self.multimedia.windowParams.width/2) as f64 + ((self.multimedia.renderParams.projPlaneDist/spriteHitDistY)*spriteHitDistX)) as i32;
            let spriteRenderHeight = (self.multimedia.renderParams.renderHeightProprConst / spriteHitDistY) as i32;
            let spriteScreenRect = Rect::new(spriteScreenX - (spriteRenderHeight/2), (self.multimedia.windowParams.height as i32)/2 - (spriteRenderHeight/2), spriteRenderHeight as u32, spriteRenderHeight as u32);
            let spriteTextureHandle = sprite.textureHandle;

            self.spritesRenderDataBuffer.push(SpriteRenderData {
                vecToSprite,
                spriteHitDistY,
                spriteHitDistX,
                spriteScreenX,
                spriteRenderHeight,
                spriteScreenRect,
                spriteTextureHandle
            });
        }

        self.spritesRenderDataBuffer.sort_by(|a, b| a.spriteRenderHeight.partial_cmp(&b.spriteRenderHeight).unwrap());

        for s in &self.spritesRenderDataBuffer {
            for x in s.spriteScreenRect.x..(s.spriteScreenRect.x+s.spriteScreenRect.w) {
                if x < 0 {
                    continue;
                } else if x >= self.multimedia.windowParams.width as i32 {
                    break;
                } else {
                    if self.wallRenderHeights[x as usize] <= s.spriteRenderHeight {
                        let spriteTextureWidthPercent = (x - s.spriteScreenRect.x) as f64 / (s.spriteScreenRect.w) as f64;
                        let spriteTextureX = (spriteTextureWidthPercent * TEXTURE_PITCH as f64) as i32;
                        let spriteTextureRect = Rect::new(spriteTextureX, 0, 1, TEXTURE_PITCH);
                        let screenRect = Rect::new(x, s.spriteScreenRect.y, 1, s.spriteScreenRect.h as u32);
                        
                        let texture = self.multimedia.assets.GetTexture(s.spriteTextureHandle);

                        let _ = self.multimedia.sdlCanvas.copy(texture, spriteTextureRect, screenRect);
                    }
                }
            }
        }
    }

    fn DrawWeapon(&mut self) {
        let screenRect = Rect::new(self.weaponRenderTopLeft.x(), self.weaponRenderTopLeft.y(), self.weaponRenderPitch as u32, self.weaponRenderPitch as u32);
        let textureHandle = self.player.AM_weapon.GetCurrTexture();
        let texture = self.multimedia.assets.GetTexture(textureHandle);
        let textureRect = Rect::new(0, 0, TEXTURE_PITCH, TEXTURE_PITCH);
        let _ = self.multimedia.sdlCanvas.copy(texture, textureRect, screenRect);
    }

    fn ResetSpriteTileHitMap(&mut self) {
        for x in 0..self.map.width {
            for y in 0..self.map.height {
                self.spriteTileHitMap[x as usize][y as usize] = false;
            }
        }
    }

    fn ResetWallRenderHeights(&mut self) {
        for i in 0..self.wallRenderHeights.len() {
            self.wallRenderHeights[i] = 0;
        }
    }

    fn GrabSprites(&mut self, tileCoord: iPoint2) {
        let currTileX = tileCoord.x() as usize;
        let currTileY = tileCoord.y() as usize;
        if self.spriteTileHitMap[currTileX][currTileY] == false {
            match self.map.GetMutTile(tileCoord) {
                Tile::OBJECT(objectTile) => {
                    self.spritesBuffer.push(objectTile.objectSprite);
                    if objectTile.IsEnemyHolder() {
                        let spritesArr = objectTile.GetSprites();
                        if spritesArr.is_some() {
                            for s in spritesArr.unwrap() {
                                self.spritesBuffer.push(*s);
                            }
                        }
                    }
                },
                Tile::EMPTY(emptyTile) => {
                    let spritesArr = emptyTile.GetSprites();
                    if spritesArr.is_some() {
                        for s in spritesArr.unwrap() {
                            self.spritesBuffer.push(*s);
                        }
                    }
                }
                _ => panic!()
            }
            self.spriteTileHitMap[currTileX][currTileY] = true;
        }
    }

    fn UpdateEnemies(&mut self) {
        // Wipe all enemy sprites from tiles
        for e in &self.enemies {
            match self.map.GetMutTile(e.tile) {
                Tile::OBJECT(object) => {
                    object.enemySprites.clear();
                },
                Tile::EMPTY(emptyTile) => {
                    emptyTile.enemySprites.clear();
                },
                Tile::NONE => panic!(),
                _ => {}
            }
        }

        // For each enemy...
        for e in &mut self.enemies {

            e.Update(&self.map, &self.player);

            // Calculate and inject sprites into appropriate tiles
            let tileCoord: iPoint2 = e.location.into();
            let sprite = e.CalculateSprite(self.player.viewDir);
            match self.map.GetMutTile(tileCoord) {
                Tile::OBJECT(object) => {
                    object.enemySprites.push(sprite);
                },
                Tile::EMPTY(emptyTile) => {
                    emptyTile.enemySprites.push(sprite);
                },
                _ => {}
            }
        }
    }
}

