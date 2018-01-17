use module::Module;
use format::FormatPlayer;
use player::{PlayerData, Virtual};
use format::protracker::{ModPatterns, ModInstrument};
use util;
use ::*;

/// Vinterstigen PT2.1A Replayer
///
/// An oxdz player based on the Protracker V2.1A play routine written by Peter
/// "CRAYON" Hanning / Mushroom Studios in 1992. Original names are used whenever
/// possible (converted to snake case according to Rust convention, i.e.
/// mt_PosJumpFlag becomes mt_pos_jump_flag).

pub struct ModPlayer {
    name : &'static str,
    state: Vec<ChannelData>,

    mt_speed          : u8,
    mt_counter        : u8,
    mt_song_pos       : u8,
    mt_pbreak_pos     : u8,
    mt_pos_jump_flag  : bool,
    mt_pbreak_flag    : bool,
    mt_low_mask       : u8,
    mt_patt_del_time  : u8,
    mt_patt_del_time_2: u8,
    mt_pattern_pos    : u8,
}

impl ModPlayer {
    pub fn new(module: &Module) -> Self {
        ModPlayer {
            name : r#""Vinterstigen" 0.1 PT2.1A replayer"#,
            state: vec![ChannelData::new(); module.chn],

            mt_speed          : 6,
            mt_counter        : 0,
            mt_song_pos       : 0,
            mt_pbreak_pos     : 0,
            mt_pos_jump_flag  : false,
            mt_pbreak_flag    : false,
            mt_low_mask       : 0,
            mt_patt_del_time  : 0,
            mt_patt_del_time_2: 0,
            mt_pattern_pos    : 0,
        }
    }

    fn mt_music(&mut self, module: &Module, mut virt: &mut Virtual) {
        let pats = module.patterns.as_any().downcast_ref::<ModPatterns>().unwrap();

        self.mt_counter += 1;
        if self.mt_speed > self.mt_counter {
            // mt_NoNewNote
            self.mt_no_new_all_channels(&module, &pats, &mut virt);
            self.mt_no_new_pos_yet(&module);
            return;
        }

        self.mt_counter = 0;
        if self.mt_patt_del_time_2 == 0 {
            self.mt_get_new_note(&module, &pats, &mut virt);
        } else {
            self.mt_no_new_all_channels(&module, &pats, &mut virt);
        }

        // mt_dskip
        self.mt_pattern_pos +=1;
        if self.mt_patt_del_time != 0 {
            self.mt_patt_del_time_2 = self.mt_patt_del_time;
            self.mt_patt_del_time = 0;
        }

        // mt_dskc
        if self.mt_patt_del_time_2 != 0 {
            self.mt_patt_del_time_2 -= 1;
            if self.mt_patt_del_time_2 != 0 {
                self.mt_pattern_pos -= 1;
            }
        }

        // mt_dska
        if self.mt_pbreak_flag {
            self.mt_pbreak_flag = false;
            self.mt_pattern_pos = self.mt_pbreak_pos;
            self.mt_pbreak_pos = 0;
        }

        // mt_nnpysk
        if self.mt_pattern_pos >= 64 {
            self.mt_next_position(&module);
        }
        self.mt_no_new_pos_yet(&module);
    }

    fn mt_no_new_all_channels(&mut self, module: &Module, pats: &ModPatterns, mut virt: &mut Virtual) {
        for chn in 0..module.chn {
            let p = module.orders.pattern(self.mt_song_pos as usize);
            let event = pats.event(p, self.mt_pattern_pos, chn);
            self.mt_check_efx(chn, &mut virt);
        }
    }

    fn mt_get_new_note(&mut self, module: &Module, pats: &ModPatterns, mut virt: &mut Virtual) {
        let p = module.orders.pattern(self.mt_song_pos as usize);
        for chn in 0..module.chn {
            let event = pats.event(p, self.mt_pattern_pos, chn);
            let (note, ins, cmd, cmdlo) = (event.note, event.ins, event.cmd, event.cmdlo);

            // mt_PlayVoice
            if self.state[chn].n_note == 0 {
                self.per_nop(chn, &mut virt);
            }

            // mt_plvskip
            self.state[chn].n_note = note;
            self.state[chn].n_ins = ins;
            self.state[chn].n_cmd = cmd;
            self.state[chn].n_cmdlo = cmdlo;

            if ins != 0 {
                let instrument = &module.instrument[ins as usize];
                let subins = instrument.subins[0].as_any().downcast_ref::<ModInstrument>().unwrap();
                let sample = &module.sample[ins as usize];
                //self.state[chn].n_start = sample.loop_start;
                //self.state[chn].n_length = sample.size;
                //self.state[chn].n_reallength = sample.size;
                self.state[chn].n_finetune = subins.finetune;
                //self.state[chn].n_replen = sample.loop_end - sample.loop_start;
                self.state[chn].n_volume = instrument.volume as u8;
                virt.set_patch(chn, ins as usize - 1, ins as usize - 1, note as usize);
                virt.set_volume(chn, instrument.volume);  // MOVE.W  D0,8(A5)        ; Set volume
            }

            // mt_SetRegs
            if note != 0 {

                match cmd {
                    0xe => if (cmdlo & 0xf0) == 0x50 {
                               // mt_DoSetFinetune
                               self.mt_set_finetune(chn);
                               self.mt_set_period(chn, &mut virt);
                           },
                    0x3 => {  // TonePortamento
                               self.mt_set_tone_porta(chn, &mut virt);
                               self.mt_check_more_efx(chn, &mut virt)
                           },
                    0x5 => {
                               self.mt_set_tone_porta(chn, &mut virt);
                               self.mt_check_more_efx(chn, &mut virt)
                           },
                    0x9 => {  // Sample Offset
                               self.mt_check_more_efx(chn, &mut virt);
                               self.mt_set_period(chn, &mut virt);
                           },
                    _   => {
                               self.mt_set_period(chn, &mut virt);
                           },
                }

            } else {
                self.mt_check_more_efx(chn, &mut virt);  // If no note
            }
        }
    }

    fn mt_set_period(&mut self, chn: usize, mut virt: &mut Virtual) {
        {
            let state = &mut self.state[chn];
            let period = util::note_to_period(state.n_note as usize, state.n_finetune, PeriodType::Amiga);
            state.n_period = period;
    
            if state.n_cmd != 0x0e || (state.n_cmdlo & 0xf0) != 0xd0 {  // !Notedelay
                // check vibrato, tremolo wave
            }
        }

        self.mt_check_more_efx(chn, &mut virt);
    }

    fn mt_next_position(&mut self, module: &Module) {
        self.mt_pattern_pos = self.mt_pbreak_pos;
        self.mt_pbreak_pos = 0;
        self.mt_pos_jump_flag = false;
        self.mt_song_pos += 1;
        self.mt_song_pos &= 0x7f;
        if self.mt_song_pos as usize >= module.len(0) {
            self.mt_song_pos = 0;
        }
    }

    fn mt_no_new_pos_yet(&mut self, module: &Module) {
        if self.mt_pos_jump_flag {
            self.mt_next_position(&module);
            self.mt_no_new_pos_yet(&module);
        }
    }

    fn mt_check_efx(&mut self, chn: usize, mut virt: &mut Virtual) {

        let cmd = self.state[chn].n_cmd;

        // mt_UpdateFunk()
        if cmd == 0 {
            self.per_nop(chn, &mut virt);
            return
        }

        match cmd {
            0x0 => self.mt_arpeggio(chn, &mut virt),
            0x1 => self.mt_porta_up(chn, &mut virt),
            0x2 => self.mt_porta_down(chn, &mut virt),
            0x3 => self.mt_tone_portamento(chn, &mut virt),
            0x4 => self.mt_vibrato(chn, &mut virt),
            0x5 => self.mt_tone_plus_vol_slide(chn, &mut virt),
            0x6 => self.mt_vibrato_plus_vol_slide(chn, &mut virt),
            0xe => self.mt_e_commands(chn, &mut virt),
            _   => {
                       // SetBack
                       virt.set_period(chn, self.state[chn].n_period);  // MOVE.W  n_period(A6),6(A5)
                       match cmd {
                           0x7 => self.mt_tremolo(chn, &mut virt),
                           0xa => self.mt_volume_slide(chn, &mut virt),
                           _   => {},
                       }
                   }
        }
    }

    fn per_nop(&self, chn: usize, virt: &mut Virtual) {
        let period = self.state[chn].n_period;
        virt.set_period(chn, period);  // MOVE.W  n_period(A6),6(A5)
    }

    fn mt_arpeggio(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        let val = match self.mt_counter % 3 {
            0 => {  // Arpeggio2
                     0
                 },
            1 => {  // Arpeggio1
                     state.n_cmdlo & 15
                 },
            _ => {  // Arpeggio3
                     state.n_cmdlo >> 4
                 },
        } as usize;
        // Arpeggio4
        let period = util::note_to_period(state.n_note as usize+ val, state.n_finetune, PeriodType::Amiga);
        virt.set_period(chn, period);  // MOVE.W  D2,6(A5)
    }

    fn mt_fine_porta_up(&mut self, chn: usize, mut virt: &mut Virtual) {
        if self.mt_counter != 0 {
            return
        }
        self.mt_low_mask = 0x0f;
        self.mt_porta_up(chn, &mut virt);
    }

    fn mt_porta_up(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        state.n_period -= (state.n_cmdlo & self.mt_low_mask) as f64;
        self.mt_low_mask = 0xff;
        if state.n_period < 113.0 {
            state.n_period = 113.0;
        }
        virt.set_period(chn, state.n_period);  // MOVE.W  n_period(A6),6(A5)
    }

    fn mt_fine_porta_down(&mut self, chn: usize, mut virt: &mut Virtual) {
        if self.mt_counter != 0 {
            return
        }
        self.mt_low_mask = 0x0f;
        self.mt_porta_down(chn, &mut virt);
    }

    fn mt_porta_down(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        state.n_period += (state.n_cmdlo & self.mt_low_mask) as f64;
        self.mt_low_mask = 0xff;
        if state.n_period < 856.0 {
            state.n_period = 856.0;
        }
        virt.set_period(chn, state.n_period);  // MOVE.W  D0,6(A5)
    }

    fn mt_set_tone_porta(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        let wantedperiod = util::note_to_period(state.n_note as usize, state.n_finetune, PeriodType::Amiga);
        state.n_wantedperiod = Some(wantedperiod);
        state.n_toneportdirec = state.n_period < wantedperiod;
    }

    fn mt_clear_tone_porta(&mut self, chn: usize) {
        self.state[chn].n_wantedperiod = None;
    }

    fn mt_tone_portamento(&mut self, chn: usize, mut virt: &mut Virtual) {
        {
            let state = &mut self.state[chn];
            if state.n_cmdlo != 0 {
                state.n_toneportspeed = state.n_cmdlo;
                state.n_cmdlo = 0;
            }
        }
        self.mt_tone_port_no_change(chn, &mut virt);
    }

    fn mt_tone_port_no_change(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        let wantedperiod = try_option!(state.n_wantedperiod);
        if state.n_toneportdirec {
            // mt_TonePortaUp
            state.n_period -= state.n_toneportspeed as f64;
            if state.n_period < wantedperiod {
                state.n_period = wantedperiod;
                state.n_wantedperiod = None;
            }
        } else {
            // mt_TonePortaDown
            state.n_period += state.n_toneportspeed as f64;
            if state.n_period > wantedperiod {
                state.n_period = wantedperiod;
                state.n_wantedperiod = None;
            }
        }
        // mt_TonePortaSetPer
        if state.n_glissfunk & 0x0f != 0 {
        }
        // mt_GlissSkip
        virt.set_period(chn, state.n_period);
    }

    fn mt_vibrato(&mut self, chn: usize, mut virt: &mut Virtual) {
        {
            let state = &mut self.state[chn];
            if state.n_cmdlo != 0 {
                if state.n_cmdlo & 0x0f != 0 {
                    state.n_vibratocmd = (state.n_vibratocmd & 0xf0) | (state.n_cmdlo & 0x0f)
                }
                // mt_vibskip
                if state.n_cmdlo & 0xf0 != 0 {
                    state.n_vibratocmd = (state.n_vibratocmd & 0x0f) | (state.n_cmdlo & 0xf0)
                }
                // mt_vibskip2
            }
        }
        self.mt_vibrato_2(chn, &mut virt);
    }

    fn mt_vibrato_2(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        let mut pos = (state.n_vibratopos >> 2) & 0x1f;
        let val = match state.n_wavecontrol & 0x03 {
            0 => {  // mt_vib_sine
                     MT_VIBRATO_TABLE[pos as usize]
                 },
            1 => {  // mt_vib_rampdown
                     pos <<= 3;
                     if pos & 0x80 != 0 { 255 - pos } else { pos }
                 },
            _ => {
                     255
                 }
        };
        // mt_vib_set
        let mut period = state.n_period;
        let amt = (val as usize * (state.n_vibratocmd & 15) as usize) >> 7;
        if state.n_vibratopos & 0x80 == 0 {
            period += amt as f64
        } else {
            period -= amt as f64
        };

        // mt_Vibrato3
        virt.set_period(chn, period);
        state.n_vibratopos = state.n_vibratopos.wrapping_add((state.n_vibratocmd >> 2) & 0x3c);
    }

    fn mt_tone_plus_vol_slide(&mut self, chn: usize, mut virt: &mut Virtual) {
        self.mt_tone_port_no_change(chn, &mut virt);
        self.mt_volume_slide(chn, &mut virt);
    }

    fn mt_vibrato_plus_vol_slide(&mut self, chn: usize, mut virt: &mut Virtual) {
        self.mt_vibrato_2(chn, &mut virt);
        self.mt_volume_slide(chn, &mut virt);
    }

    fn mt_tremolo(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        if state.n_cmdlo != 0 {
            if state.n_cmdlo & 0x0f != 0 {
                 state.n_tremolocmd = (state.n_cmdlo & 0x0f) | (state.n_tremolocmd & 0xf0)
            }
            // mt_treskip
            if state.n_cmdlo & 0xf0 != 0 {
                 state.n_tremolocmd = (state.n_cmdlo & 0xf0) | (state.n_tremolocmd & 0x0f)
            }
            // mt_treskip2
        }
        // mt_Tremolo2
        let mut pos = (state.n_tremolopos >> 2) & 0x1f;
        let val = match (state.n_wavecontrol >> 4) & 0x03 {
            0 => {  // mt_tre_sine
                     MT_VIBRATO_TABLE[pos as usize]
                 },
            1 => {  // mt_rampdown
                     pos <<= 3;
                     if pos & 0x80 != 0 { 255 - pos } else { pos }
                 },
            _ => {
                     255
                 },
        };
        // mt_tre_set
        let mut volume = state.n_volume as isize;
        let amt = ((val as usize * (state.n_tremolocmd & 15) as usize) >> 6) as isize;
        if state.n_tremolopos & 0x80 == 0 {
            volume += amt;
        } else {
            volume -= amt;
        }
        // mt_Tremolo3
        if volume < 0 {
            volume = 0;
        }
        // mt_TremoloSkip
        if volume > 0x40 {
           volume = 0x40;
        }

        // mt_TremoloOk
        virt.set_volume(chn, volume as usize);  // MOVE.W  D0,8(A5)
        state.n_tremolopos = state.n_tremolopos.wrapping_add((state.n_tremolocmd >> 2) & 0x3c);
    }

    fn mt_sample_offset(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        if state.n_cmdlo != 0 {
            state.n_sampleoffset = state.n_cmdlo;
        }
        virt.set_voicepos(chn, (state.n_sampleoffset << 7) as f64);

    }

    fn mt_volume_slide(&mut self, chn: usize, mut virt: &mut Virtual) {
        if self.state[chn].n_cmdlo & 0xf0 == 0 {
            self.mt_vol_slide_down(chn, &mut virt);
        } else {
            self.mt_vol_slide_up(chn, &mut virt);
        }
    }

    fn mt_vol_slide_up(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        state.n_volume += state.n_cmdlo & 0x0f;
        if state.n_volume > 0x40 {
            state.n_volume = 0x40;
        }
        virt.set_volume(chn, state.n_volume as usize);
    }

    fn mt_vol_slide_down(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        let cmdlo = state.n_cmdlo & 0x0f;
        if state.n_volume > cmdlo {
            state.n_volume -= cmdlo;
        } else {
            state.n_volume = 0;
        }
        virt.set_volume(chn, state.n_volume as usize);
    }

    fn mt_position_jump(&mut self, chn: usize) {
        let state = &mut self.state[chn];
        self.mt_song_pos = state.n_cmdlo - 1;
        // mt_pj2
        self.mt_pbreak_pos = 0;
        self.mt_pos_jump_flag = true;
    }

    fn mt_volume_change(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        if state.n_cmdlo > 0x40 {
            state.n_cmdlo = 40
        }
        state.n_volume = state.n_cmdlo;
        virt.set_volume(chn, state.n_volume as usize);  // MOVE.W  D0,8(A5)
    }

    fn mt_pattern_break(&mut self, chn: usize) {
        let state = &mut self.state[chn];
        let line = (state.n_cmdlo >> 4) * 10 + (state.n_cmdlo & 0x0f);
        if line >= 63 {
            // mt_pj2
            self.mt_pbreak_pos = 0;
        }
        self.mt_pos_jump_flag = true;
    }

    fn mt_set_speed(&mut self, chn: usize) {
        let state = &mut self.state[chn];
        if state.n_cmdlo != 0 {
            self.mt_counter = 0;
            self.mt_speed = state.n_cmdlo;
        }
    }

    fn mt_check_more_efx(&mut self, chn: usize, mut virt: &mut Virtual) {
        // mt_UpdateFunk()

        match self.state[chn].n_cmd {
            0x9 => self.mt_sample_offset(chn, &mut virt),
            0xb => self.mt_position_jump(chn),
            0xd => self.mt_pattern_break(chn),
            0xe => self.mt_e_commands(chn, &mut virt),
            0xf => self.mt_set_speed(chn),
            0xc => self.mt_volume_change(chn, &mut virt),
            _   => {},
        }

        // per_nop
        self.per_nop(chn, &mut virt)
    }

    fn mt_e_commands(&mut self, chn: usize, mut virt: &mut Virtual) {

        match self.state[chn].n_cmdlo >> 4 {
           0x0 => self.mt_filter_on_off(chn, &mut virt),
           0x1 => self.mt_fine_porta_up(chn, &mut virt),
           0x2 => self.mt_fine_porta_down(chn, &mut virt),
           0x3 => self.mt_set_gliss_control(chn),
           0x4 => self.mt_set_vibrato_control(chn),
           0x5 => self.mt_set_finetune(chn),
           0x6 => self.mt_jump_loop(chn),
           0x7 => self.mt_set_tremolo_control(chn),
           0x9 => self.mt_retrig_note(chn, &mut virt),
           0xa => self.mt_volume_fine_up(chn, &mut virt),
           0xb => self.mt_volume_fine_down(chn, &mut virt),
           0xc => self.mt_note_cut(chn, &mut virt),
           0xd => self.mt_note_delay(chn, &mut virt),
           0xe => self.mt_pattern_delay(chn),
           0xf => self.mt_funk_it(chn, &mut virt),
           _   => {},
        }
    }

    fn mt_filter_on_off(&self, _chn: usize, mut _virt: &mut Virtual) {
    }

    fn mt_set_gliss_control(&mut self, chn: usize) {
        let state = &mut self.state[chn];
        state.n_glissfunk = state.n_cmdlo;
    }

    fn mt_set_vibrato_control(&mut self, chn: usize) {
        let state = &mut self.state[chn];
        state.n_wavecontrol &= 0xf0;
        state.n_wavecontrol |= state.n_cmdlo & 0x0f;
    }

    fn mt_set_finetune(&mut self, chn: usize) {
        let state = &mut self.state[chn];
        state.n_finetune = ((state.n_cmdlo << 4) as i8) as isize;
    }

    fn mt_jump_loop(&mut self, chn: usize) {
        let state = &mut self.state[chn];

        if self.mt_counter != 0 {
            return
        }

        let cmdlo = state.n_cmdlo & 0x0f;

        if cmdlo == 0 {
            // mt_SetLoop
            state.n_pattpos = self.mt_pattern_pos as u8;
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

    fn mt_set_tremolo_control(&mut self, chn: usize) {
        let mut state = &mut self.state[chn];
        state.n_wavecontrol &= 0x0f;
        state.n_wavecontrol |= (state.n_cmdlo & 0x0f) << 4;
    }

    fn mt_retrig_note(&self, chn: usize, mut virt: &mut Virtual) {
    }

    fn mt_volume_fine_up(&mut self, chn: usize, mut virt: &mut Virtual) {
        if self.mt_counter != 0 {
            return;
        }
        self.mt_vol_slide_up(chn, &mut virt);
    }

    fn mt_volume_fine_down(&mut self, chn: usize, mut virt: &mut Virtual) {
        if self.mt_counter != 0 {
            return;
        }
        self.mt_vol_slide_down(chn, &mut virt);
    }

    fn mt_note_cut(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        if self.mt_counter != state.n_cmdlo {
            return;
        }
        state.n_volume = 0;
        virt.set_volume(chn, 0);  // MOVE.W  #0,8(A5)
    }

    fn mt_note_delay(&mut self, chn: usize, mut virt: &mut Virtual) {
        let state = &mut self.state[chn];
        if self.mt_counter != state.n_cmdlo {
            return;
        }
        if state.n_note == 0 {
            return;
        }
        // BRA mt_DoRetrig
    }

    fn mt_pattern_delay(&mut self, chn: usize) {
        let state = &mut self.state[chn];
        if self.mt_counter != 0 {
            return;
        }
        if self.mt_patt_del_time_2 != 0 {
            return;
        }
        self.mt_patt_del_time = state.n_cmdlo & 0x0f + 1;
    }

    fn mt_funk_it(&self, chn: usize, mut virt: &mut Virtual) {
    }
}

impl FormatPlayer for ModPlayer {
    fn name(&self) -> &'static str {
        self.name
    }

    fn play(&mut self, mut data: &mut PlayerData, module: &Module, mut virt: &mut Virtual) {
        self.mt_speed = data.speed as u8;
        self.mt_song_pos = data.pos as u8;
        self.mt_pattern_pos = data.row as u8;

        if data.frame < 1 {
            self.mt_counter = self.mt_speed;
        } else {
            self.mt_counter = data.frame as u8 - 1;
        }
        
        self.mt_music(&module, &mut virt);

        data.frame = self.mt_counter as usize + 1;
        data.row = self.mt_pattern_pos as usize;
        data.pos = self.mt_song_pos as usize;
        data.speed = self.mt_speed as usize;
    }

    fn reset(&mut self) {
        self.mt_speed           = 6;
        self.mt_counter         = 0;
        self.mt_song_pos        = 0;
        self.mt_pbreak_pos      = 0;
        self.mt_pos_jump_flag   = false;
        self.mt_pbreak_flag     = false;
        self.mt_low_mask        = 0;
        self.mt_patt_del_time   = 0;
        self.mt_patt_del_time_2 = 0;
        self.mt_pattern_pos     = 0;
    }

}


#[derive(Clone,Default)]
struct ChannelData {
    n_note         : u8,
    n_ins          : u8,     // not in PT2.1A
    n_cmd          : u8,
    n_cmdlo        : u8,
    n_period       : f64,    // u16
    n_finetune     : isize,  // i8
    n_volume       : u8,
    n_toneportdirec: bool,
    n_toneportspeed: u8,
    n_wantedperiod : Option<f64>,
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

