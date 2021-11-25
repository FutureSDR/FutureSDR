import bt2
import sys
import collections


def parse():
    it = bt2.TraceCollectionMessageIterator(sys.argv[1])

    for msg in it:
        if type(msg) is not bt2._EventMessageConst:
            continue

        event = msg.event
        event_type = None

        if event.cls.name == 'null_rand_latency:tx' or event.cls.name == 'futuresdr:tx':
            event_type = 'tx'
        elif event.cls.name == 'null_rand_latency:rx' or event.cls.name == 'futuresdr:rx':
            event_type = 'rx'
        else: 
            continue

        cpu = event.packet.context_field['cpu_id']
        time = msg.default_clock_snapshot.ns_from_origin
        block = event.payload_field['block']
        samples = event.payload_field['samples']
        print(f"{time},{event_type},{cpu},{block},{samples}")


if __name__ == '__main__':
    parse()
