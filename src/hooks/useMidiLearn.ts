import { useCallback, useEffect, useRef } from 'react';
import type { PointerEvent, MouseEvent } from 'react';
import {
  armControl, clearAllBindings, clearBinding, initMidi, registerMidiHandler, releaseMidi, setLearn, useMidi,
} from '../lib/midi';

export function useMidiLearn() {
  const supported = useMidi((s) => s.supported);
  const ready = useMidi((s) => s.ready);
  const inputs = useMidi((s) => s.inputs);
  const learn = useMidi((s) => s.learn);
  const error = useMidi((s) => s.error);
  const bindings = useMidi((s) => Object.keys(s.map).length);

  const needed = learn || bindings > 0;
  useEffect(() => {
    if (needed) void initMidi();
    else releaseMidi();
  }, [needed]);

  const toggleLearn = useCallback(() => setLearn(!useMidi.getState().learn), []);
  return { supported, ready, inputs, learn, error, bindings, toggleLearn, clearAll: clearAllBindings };
}

export function useMidiControl(controlId: string | undefined, onValue: (v01: number) => void) {
  const ref = useRef(onValue);
  ref.current = onValue;
  const bound = useMidi((s) => (controlId ? controlId in s.map : false));
  const armed = useMidi((s) => (controlId ? s.armed === controlId : false));
  const learning = useMidi((s) => s.learn);

  useEffect(() => {
    if (!controlId) return;
    return registerMidiHandler(controlId, (v) => ref.current(v));
  }, [controlId]);

  const onPointerDownCapture = useCallback(
    (e: PointerEvent) => {
      if (!controlId || !useMidi.getState().learn) return;
      e.preventDefault();
      e.stopPropagation();
      armControl(controlId);
    },
    [controlId],
  );
  const onContextMenu = useCallback(
    (e: MouseEvent) => {
      if (!controlId || !useMidi.getState().learn) return;
      e.preventDefault();
      clearBinding(controlId);
    },
    [controlId],
  );

  const learnProps = controlId && learning
    ? { onPointerDownCapture, onContextMenu, 'data-midi': armed ? 'armed' : bound ? 'bound' : 'free' }
    : { 'data-midi': bound ? 'bound' : undefined };

  return { bound, armed, learning, learnProps } as const;
}
