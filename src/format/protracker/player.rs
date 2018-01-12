use module::Module;
use format::FormatPlayer;
use player::{PlayerData, Virtual};
use super::ModPatterns;

/// Vinterstigen PT2.1A Replayer
///
/// An oxdz player based on the Protracker V2.1A play routine written by Peter
/// "CRAYON" Hanning / Mushroom Studios in 1992. Original names are used whenever
/// possible (converted to snake case according to Rust convention, i.e.
/// mt_PosJumpFlag becomes mt_pos_jump_flag).

pub struct ModPlayer {
    name : &'static str,
    state: Vec<ChannelData>,

//  mt_speed          : u8,  // -> data.speed
//  mt_counter        : u8,  // -> data.frame
//  mt_song_pos       : u8,  // -> data.pos
    mt_pbreak_pos     : u8,
    mt_pos_jump_flag  : bool,
    mt_pbreak_flag    : bool,
    mt_low_mask       : u8,
    mt_patt_del_time  : u8,
    mt_patt_del_time_2: u8,
//  mt_pattern_pos    : u8,  // -> data.row
}

impl ModPlayer {
    pub fn new(module: &Module) -> Self {
        ModPlayer {
            name : r#""Vinterstigen" 0.1 PT2.1A replayer"#,
            state: vec![ChannelData::new(); module.chn],

//          mt_speed          : 0,
//          mt_counter        : 0,
//          mt_song_pos       : 0,
            mt_pbreak_pos     : 0,
            mt_pos_jump_flag  : false,
            mt_pbreak_flag    : false,
            mt_low_mask       : 0,
            mt_patt_del_time  : 0,
            mt_patt_del_time_2: 0,
        }
    }

    fn mt_music(&mut self, mut data: &mut PlayerData, module: &Module, mut virt: &mut Virtual) {
        let pats = module.patterns.as_any().downcast_ref::<ModPatterns>().unwrap();

        data.frame += 1;
        if data.frame >= data.speed {
            data.frame = 0;
            if self.mt_patt_del_time_2 == 0 {
                self.mt_get_new_note(&mut data, &module, &pats, &mut virt);
            } else {
                self.mt_no_new_all_channels(&mut data, &pats, &mut virt);

                // mt_dskip
                data.pos +=1;
                if self.mt_patt_del_time != 0 {
                    self.mt_patt_del_time_2 = self.mt_patt_del_time;
                    self.mt_patt_del_time = 0;
                }

                // mt_dskc
                if self.mt_patt_del_time_2 != 0 {
                    self.mt_patt_del_time_2 -= 1;
                    if self.mt_patt_del_time_2 != 0 {
                        data.row -= 1;
                    }
                }

                // mt_dska
                if self.mt_pbreak_flag {
                    self.mt_pbreak_flag = false;
                    data.row = self.mt_pbreak_pos as usize;
                    self.mt_pbreak_pos = 0;
                }

                // mt_nnpysk
                if data.row >= 64 {
                    self.mt_next_position(&mut data, &module);
                }
                self.mt_no_new_pos_yet(&mut data, &module);
            }
        } else {
            // mt_NoNewNote
            self.mt_no_new_all_channels(&mut data, &pats, &mut virt);
            self.mt_no_new_pos_yet(&mut data, &module);
            return;
        }
    }

    fn mt_no_new_all_channels(&mut self, data: &mut PlayerData, pats: &ModPatterns, mut virt: &mut Virtual) {
        for chn in 0..self.state.len() {
            let event = pats.event(data.pos, data.row, chn);
            let mut e = EffectData{chn, cmd: event.cmd, cmdlo: event.cmdlo, data};
            self.mt_check_efx(&mut e, &mut virt);
        }
    }

    fn mt_get_new_note(&mut self, mut data: &mut PlayerData, module: &Module, pats: &ModPatterns, mut virt: &mut Virtual) {
        for chn in 0..self.state.len() {
            // mt_PlayVoice
            let event = pats.event(data.pos, data.row, chn);
            if event.has_ins() {
                let instrument = &module.instrument[event.ins as usize];
                virt.set_patch(chn, event.ins as usize, event.ins as usize, event.note as usize);
                virt.set_volume(chn, instrument.volume);
            }

            let mut e = EffectData{chn, cmd: event.cmd, cmdlo: event.cmdlo, data};

            // mt_SetRegs
            if event.has_note() {
                let period = self.state[chn].n_period as f64;

                match event.cmd {
                    0xe => if (event.cmdlo & 0xf0) == 0x50 {
                                // mt_DoSetFinetune()
                           },
                    0x3 => {
                               self.mt_set_tone_porta(&mut e, &mut virt);
                               self.mt_check_efx(&mut e, &mut virt)
                           },
                    0x5 => {
                               self.mt_set_tone_porta(&mut e, &mut virt);
                               self.mt_check_efx(&mut e, &mut virt)
                           },
                    0x9 => {
                               self.mt_check_more_efx(&mut e, &mut virt);
                               virt.set_period(chn, period)
                           },
                    _   => virt.set_period(chn, period),
                }
                

            } else {
                self.mt_check_more_efx(&mut e, &mut virt);
            }
        }
    }

    fn mt_next_position(&mut self, mut data: &mut PlayerData, module: &Module) {
        data.row = self.mt_pbreak_pos as usize;
        self.mt_pbreak_pos = 0;
        self.mt_pos_jump_flag = false;
        data.pos += 1;
        data.pos &= 0x7f;
        if data.pos >= module.len(0) {
            data.pos = 0;
        }
    }

    fn mt_no_new_pos_yet(&mut self, mut data: &mut PlayerData, module: &Module) {
        if self.mt_pos_jump_flag {
            self.mt_next_position(&mut data, &module);
            self.mt_no_new_pos_yet(&mut data, &module);
        }
    }

    fn mt_check_efx(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {

        // mt_UpdateFunk()
        if e.cmd == 0 {
            self.per_nop(&mut e, &mut virt);
            return
        }

        match e.cmd {
            0x0 => self.mt_arpeggio(&mut e, &mut virt),
            0x1 => self.mt_porta_up(&mut e, &mut virt),
            0x2 => self.mt_porta_down(&mut e, &mut virt),
            0x3 => self.mt_tone_portamento(&mut e, &mut virt),
            0x4 => self.mt_vibrato(&mut e, &mut virt),
            0x5 => self.mt_tone_plus_vol_slide(&mut e, &mut virt),
            0x6 => self.mt_vibrato_plus_vol_slide(&mut e, &mut virt),
            0xe => self.mt_e_commands(&mut e, &mut virt),
            _   => {
                       // SetBack
                       virt.set_period(e.chn, self.state[e.chn].n_period as f64);  // MOVE.W  n_period(A6),6(A5)
                       match e.cmd {
                           0x7 => self.mt_tremolo(&mut e, &mut virt),
                           0xa => self.mt_volume_slide(&mut e, &mut virt),
                           _   => {},
                       }
                   }
        }
    }

    fn per_nop(&self, mut e: &mut EffectData, virt: &mut Virtual) {
        let period = self.state[e.chn].n_period;
        virt.set_period(e.chn, period as f64);  // MOVE.W  n_period(A6),6(A5)
    }

    fn mt_arpeggio(&self, e: &mut EffectData, mut virt: &mut Virtual) {
        match e.data.frame % 3 {
            0 => {  // Arpeggio2
                 },
            1 => {  // Arpeggio1
                 },
            2 => {  // Arpeggio3
                 },
            _ => {},
        }
    }

    fn mt_fine_porta_up(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        if e.data.frame != 0 {
            return
        }
        self.mt_low_mask = 0x0f;
        self.mt_porta_up(&mut e, &mut virt);
    }

    fn mt_porta_up(&mut self, e: &mut EffectData, mut virt: &mut Virtual) {
        let mut state = &mut self.state[e.chn];
        state.n_period -= (e.cmdlo & self.mt_low_mask) as u16;
        self.mt_low_mask = 0xff;
        if state.n_period < 113 {
            state.n_period = 113;
        }
        virt.set_period(e.chn, state.n_period as f64);  // MOVE.W  D0,6(A5)
    }

    fn mt_fine_porta_down(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        if e.data.frame != 0 {
            return
        }
        self.mt_low_mask = 0x0f;
        self.mt_porta_down(&mut e, &mut virt);
    }

    fn mt_porta_down(&mut self, e: &mut EffectData, mut virt: &mut Virtual) {
        let mut state = &mut self.state[e.chn];
        state.n_period += (e.cmdlo & self.mt_low_mask) as u16;
        self.mt_low_mask = 0xff;
        if state.n_period < 856 {
            state.n_period = 856;
        }
        virt.set_period(e.chn, state.n_period as f64);  // MOVE.W  D0,6(A5)
    }

    fn mt_set_tone_porta(&self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        let note = self.state[e.chn].n_note;
        //let period = 
    }

    fn mt_clear_tone_porta(&mut self, mut e: &mut EffectData) {
        self.state[e.chn].n_wantedperiod = 0;
    }

    fn mt_tone_portamento(&self, mut e: &mut EffectData, mut virt: &mut Virtual) {
    }

    fn mt_tone_port_no_change(&self, mut e: &mut EffectData, mut virt: &mut Virtual) {
    }

    fn mt_vibrato(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        let mut cmdlo = e.cmdlo;
        let mut vcmd = self.state[e.chn].n_vibratocmd;
        if cmdlo != 0 {
            if cmdlo & 0x0f != 0 {
                cmdlo = (vcmd & 0xf0) | (cmdlo & 0x0f)
            }
            // mt_vibskip
            if e.cmdlo & 0xf0 != 0 {
                cmdlo = (vcmd & 0x0f) | (cmdlo & 0xf0)
            }
            // mt_vibskip2
            self.state[e.chn].n_vibratocmd = cmdlo;
        }
        self.mt_vibrato_2(&mut e, &mut virt);
    }

    fn mt_vibrato_2(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        let mut state = &mut self.state[e.chn];
        let pos = (state.n_vibratopos >> 2) & 0x1f;
        match state.n_wavecontrol & 0x03 {
            0 => {  // mt_vib_sine
                 },
            1 => {  // mt_vib_rampdown
                 },
            _ => {}
        }

        //let v = MT_VIBRATO_TABLE[];
    }

    fn mt_tone_plus_vol_slide(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        self.mt_tone_port_no_change(&mut e, &mut virt);
        self.mt_volume_slide(&mut e, &mut virt);
    }

    fn mt_vibrato_plus_vol_slide(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        self.mt_vibrato_2(&mut e, &mut virt);
        self.mt_volume_slide(&mut e, &mut virt);
    }

    fn mt_tremolo(&self, mut e: &mut EffectData, mut virt: &mut Virtual) {
    }

    fn mt_sample_offset(&self, mut e: &mut EffectData, mut virt: &mut Virtual) {
    }

    fn mt_volume_slide(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        if e.cmdlo & 0xf0 == 0 {
            self.mt_vol_slide_down(&mut e, &mut virt);
        } else {
            self.mt_vol_slide_up(&mut e, &mut virt);
        }
    }

    fn mt_vol_slide_up(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        let cmdlo = e.cmdlo & 0x0f;
        let mut state = &mut self.state[e.chn];
        state.n_volume += cmdlo;
        if state.n_volume > 0x40 {
            state.n_volume = 0x40;
        }
        virt.set_volume(e.chn, state.n_volume as usize);
    }

    fn mt_vol_slide_down(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        let cmdlo = e.cmdlo & 0x0f;
        let mut state = &mut self.state[e.chn];
        if state.n_volume > cmdlo {
            state.n_volume -= cmdlo;
        } else {
            state.n_volume = 0;
        }
        virt.set_volume(e.chn, state.n_volume as usize);
    }

    fn mt_position_jump(&mut self, e: &mut EffectData) {
        e.data.pos = e.cmdlo as usize - 1;
        // mt_pj2
        self.mt_pbreak_pos = 0;
        self.mt_pos_jump_flag = true;
    }

    fn mt_volume_change(&mut self, e: &mut EffectData, mut virt: &mut Virtual) {
        if e.cmdlo > 0x40 {
            e.cmdlo = 40
        }
        self.state[e.chn].n_volume = e.cmdlo;
        virt.set_volume(e.chn, e.cmdlo as usize);  // MOVE.W  D0,8(A5)
    }

    fn mt_pattern_break(&mut self, e: &mut EffectData) {
        let line = (e.cmdlo >> 4) * 10 + (e.cmdlo & 0x0f);
        if line >= 63 {
            // mt_pj2
            self.mt_pbreak_pos = 0;
        }
        self.mt_pos_jump_flag = true;
    }

    fn mt_set_speed(&self, e: &mut EffectData) {
        if e.cmdlo != 0 {
            e.data.frame = 0;
            e.data.speed = e.cmdlo as usize;
        }
    }

    fn mt_check_more_efx(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        let cmd = 0;

        // mt_UpdateFunk()
        match cmd {
            0x9 => self.mt_sample_offset(&mut e, &mut virt),
            0xb => self.mt_position_jump(&mut e),
            0xd => self.mt_pattern_break(&mut e),
            0xe => self.mt_e_commands(&mut e, &mut virt),
            0xf => self.mt_set_speed(&mut e),
            0xc => self.mt_volume_change(&mut e, &mut virt),
            _   => {},
        }

        // per_nop
        self.per_nop(&mut e, &mut virt)
    }

    fn mt_e_commands(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {

        match e.cmdlo >> 4 {
           0x0 => self.mt_filter_on_off(&mut e, &mut virt),
           0x1 => self.mt_fine_porta_up(&mut e, &mut virt),
           0x2 => self.mt_fine_porta_down(&mut e, &mut virt),
           0x3 => self.mt_set_gliss_control(&mut e),
           0x4 => self.mt_set_vibrato_control(&mut e),
           0x5 => self.mt_set_finetune(&mut e),
           0x6 => self.mt_jump_loop(&mut e),
           0x7 => self.mt_set_tremolo_control(&mut e),
           0x9 => self.mt_retrig_note(&mut e, &mut virt),
           0xa => self.mt_volume_fine_up(&mut e, &mut virt),
           0xb => self.mt_volume_fine_down(&mut e, &mut virt),
           0xc => self.mt_note_cut(&mut e, &mut virt),
           0xd => self.mt_note_delay(&mut e, &mut virt),
           0xe => self.mt_pattern_delay(&mut e),
           0xf => self.mt_funk_it(&mut e, &mut virt),
           _   => {},
        }
    }

    fn mt_filter_on_off(&self, mut e: &mut EffectData, mut virt: &mut Virtual) {
    }

    fn mt_set_gliss_control(&mut self, mut e: &mut EffectData) {
        self.state[e.chn].n_glissfunk = e.cmdlo;
    }

    fn mt_set_vibrato_control(&mut self, mut e: &mut EffectData) {
        let mut state = &mut self.state[e.chn];
        state.n_wavecontrol &= 0xf0;
        state.n_wavecontrol |= e.cmdlo & 0x0f;
    }

    fn mt_set_finetune(&mut self, mut e: &mut EffectData) {
        self.state[e.chn].n_finetune = e.cmdlo as i8;
    }

    fn mt_jump_loop(&mut self, mut e: &mut EffectData) {
        if e.data.frame != 0 {
            return
        }

        let cmdlo = e.cmdlo & 0x0f;
        let mut state = &mut self.state[e.chn];

        if cmdlo == 0 {
            // mt_SetLoop
            state.n_pattpos = e.data.row as u8;
        } else {
            if state.n_loopcount == 0 {
                // mt_jmpcnt
                state.n_loopcount = cmdlo;
            } else {
                state.n_loopcount -= 1;
                if state.n_loopcount == 0 {
                    return;
                }
            }
            // mt_jmploop
            self.mt_pbreak_pos = state.n_pattpos;
            self.mt_pbreak_flag = true;
        }
    }

    fn mt_set_tremolo_control(&mut self, mut e: &mut EffectData) {
        let mut state = &mut self.state[e.chn];
        state.n_wavecontrol &= 0x0f;
        state.n_wavecontrol |= (e.cmdlo & 0x0f) << 4;
    }

    fn mt_retrig_note(&self, mut e: &mut EffectData, mut virt: &mut Virtual) {
    }

    fn mt_volume_fine_up(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        if e.data.frame != 0 {
            return;
        }
        e.cmdlo &= 0x0f;
        self.mt_vol_slide_up(&mut e, &mut virt);
    }

    fn mt_volume_fine_down(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        if e.data.frame != 0 {
            return;
        }
        e.cmdlo &= 0x0f;
        self.mt_vol_slide_down(&mut e, &mut virt);
    }

    fn mt_note_cut(&mut self, e: &mut EffectData, mut virt: &mut Virtual) {
        let mut state = &mut self.state[e.chn];
        if e.data.frame != e.cmdlo as usize {
            return;
        }
        state.n_volume = 0;
        virt.set_volume(e.chn, 0);  // MOVE.W  #0,8(A5)
    }

    fn mt_note_delay(&mut self, mut e: &mut EffectData, mut virt: &mut Virtual) {
        let cmdlo = e.cmdlo & 0x0f;
        let mut state = &mut self.state[e.chn];
        if e.data.frame != cmdlo as usize {
            return;
        }
        if state.n_note == 0 {
            return;
        }
        // BRA mt_DoRetrig
    }

    fn mt_pattern_delay(&mut self, e: &mut EffectData) {
        if e.data.frame != 0 {
            return;
        }
        if self.mt_patt_del_time_2 != 0 {
            return;
        }
        self.mt_patt_del_time = e.cmdlo & 0x0f + 1;
    }

    fn mt_funk_it(&self, e: &mut EffectData, mut virt: &mut Virtual) {
    }
}

impl FormatPlayer for ModPlayer {
    fn name(&self) -> &'static str {
        self.name
    }

    fn play(&mut self, mut data: &mut PlayerData, module: &Module, mut virt: &mut Virtual) {
        self.mt_music(&mut data, &module, &mut virt)
    }

    fn reset(&mut self) {
        self.mt_pbreak_pos      = 0;
        self.mt_pos_jump_flag   = false;
        self.mt_pbreak_flag     = false;
        self.mt_low_mask        = 0;
        self.mt_patt_del_time   = 0;
        self.mt_patt_del_time_2 = 0;
    }
}


#[derive(Clone,Default)]
struct ChannelData {
    n_note         : u8,
    n_cmd          : u8,
    n_cmdlo        : u8,
    n_period       : u16,
    n_finetune     : i8,
    n_volume       : u8,
    n_toneportdirec: i8,
    n_toneportspeed: u8,
    n_wantedperiod : u16,
    n_vibratocmd   : u8,
    n_vibratopos   : u8,
    n_tremolocmd   : u8,
    n_tremolopos   : u8,
    n_wavecontrol  : u8,
    n_glissfunk    : u8,
    n_sampleoffset : u8,
    n_pattpos      : u8,
    n_loopcount    : u8,
    n_funkoffset   : u8,
    n_wavestart    : u32,
    n_reallength   : u16,
}

impl ChannelData {
    pub fn new() -> Self {
        Default::default()
    }
}


const MT_FUNK_TABLE: &'static [u8] = &[
    0, 5, 6, 7, 8, 10, 11, 13, 16, 19, 22, 26, 32, 43, 64, 128
];

const MT_VIBRATO_TABLE: &'static [u8] = &[
      0,  24,  49,  74,  97, 120, 141, 161,
    180, 197, 212, 224, 235, 244, 250, 253,
    255, 253, 250, 244, 235, 224, 212, 197,
    180, 161, 141, 120,  97,  74,  49,  24
];


struct EffectData<'a> {
    chn  : usize,
    cmd  : u8,
    cmdlo: u8,
    data : &'a mut PlayerData,
}

