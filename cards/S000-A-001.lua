-- Test A
define_card(
    "S000-A-001",
    function(card)
            -- base attribute
            card:name("测试卡001")
            card:cost(2)
            card:ack(100)
            -- effects
            card:reg_effect(
                "e1",
                function(effect)
                    -- 登时抽卡
                    effect:window("set")
                    effect:draw(1)
            end)
end)