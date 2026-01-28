use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use tjiftjaf::{
    ConnAck, Connect, Disconnect, Packet, PubAck, Publish, QoS, SubAck, Subscribe, UnsubAck,
    Unsubscribe,
};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("decode/encode Connect ", |b| {
        let packet: Packet = Connect::builder()
            .username("admin")
            .password("secret")
            .build()
            .into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });

    c.bench_function("decode/encode ConnAck ", |b| {
        let packet: Packet = ConnAck::builder().build().into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });

    c.bench_function("decode/encode Subscribe", |b| {
        let packet: Packet = Subscribe::builder("sensors/temperature/1", QoS::AtMostOnceDelivery)
            .add_topic("sensors/humidity/2", QoS::AtMostOnceDelivery)
            .build()
            .into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });

    c.bench_function("decode/encode SubAck", |b| {
        let packet: Packet = SubAck::builder(1337, QoS::AtMostOnceDelivery)
            .add_return_code(QoS::AtMostOnceDelivery)
            .build()
            .into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });
    c.bench_function("decode/encode Publish", |b| {
        let packet: Packet = Publish::builder("sensors/temperature/1", r#"{"measurement": 19.2}"#)
            .build()
            .into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });

    c.bench_function("decode/encode PubAck", |b| {
        let packet: Packet = PubAck::new(1337).into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });

    c.bench_function("decode/encode Unsubscribe", |b| {
        let packet: Packet = Unsubscribe::builder("sensors/temperature/1")
            .add_topic("sensors/humidity/1")
            .build()
            .into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });
    c.bench_function("decode/encode UnsubAck", |b| {
        let packet: Packet = UnsubAck::new(1337).into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });

    c.bench_function("decode/encode Disconnect", |b| {
        let packet: Packet = Disconnect.into();

        b.iter(|| {
            Packet::try_from(black_box(packet.clone().into_bytes()))
                .unwrap()
                .into_bytes()
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
